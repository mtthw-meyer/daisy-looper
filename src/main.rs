#![no_main]
#![no_std]
use core::convert::TryInto;
use core::{mem, slice};
use log::info;

use stm32h7xx_hal::stm32;
use stm32h7xx_hal::timer::Timer;

use libdaisy::audio;
use libdaisy::gpio::*;
use libdaisy::hid;
use libdaisy::logger;
use libdaisy::prelude::*;
use libdaisy::system;

use daisy_looper::*;

// const LOOP_BUFFFER_SIZE: usize = 64 * 1024 * 1024 / 2 / mem::size_of::<u32>();
const LOOP_BUFFFER_SIZE: usize = libdaisy::sdram::Sdram::bytes() / 2 / mem::size_of::<f32>();

#[rtic::app(
    device = stm32h7xx_hal::stm32,
    peripherals = true,
    monotonic = rtic::cyccnt::CYCCNT,
)]
const APP: () = {
    struct Resources {
        audio: audio::Audio,
        buffer: audio::AudioBuffer,
        seed_led: hid::Led<SeedLed>,
        switch1: hid::Switch<Daisy28<Input<PullUp>>>,
        looper: Looper<LOOP_BUFFFER_SIZE>,
        timer2: Timer<stm32::TIM2>,
    }

    #[init]
    fn init(ctx: init::Context) -> init::LateResources {
        logger::init();
        let mut system = system::System::init(ctx.core, ctx.device);
        let buffer = [(0.0, 0.0); audio::BLOCK_SIZE_MAX];
        system.timer2.set_freq(1.ms());

        let loop_buffer_1: &mut [f32; LOOP_BUFFFER_SIZE] = unsafe {
            slice::from_raw_parts_mut(&mut system.sdram[0], LOOP_BUFFFER_SIZE)
                .try_into()
                .unwrap()
        };
        let loop_buffer_2: &mut [f32; LOOP_BUFFFER_SIZE] = unsafe {
            slice::from_raw_parts_mut(&mut system.sdram[LOOP_BUFFFER_SIZE], LOOP_BUFFFER_SIZE)
                .try_into()
                .unwrap()
        };

        let mut seed_led = hid::Led::new(system.gpio.led, false, 1000);
        seed_led.set_brightness(0.0);

        let daisy28 = system
            .gpio
            .daisy28
            .take()
            .expect("Failed to get pin daisy28!")
            .into_pull_up_input();

        let looper = Looper::new(loop_buffer_1, Some(loop_buffer_2));

        let mut switch1 = hid::Switch::new(daisy28, hid::SwitchType::PullUp);
        switch1.set_double_thresh(Some(500));
        switch1.set_held_thresh(Some(1500));

        info!("Startup done!");

        init::LateResources {
            audio: system.audio,
            buffer,
            seed_led,
            switch1,
            looper,
            timer2: system.timer2,
        }
    }

    // Interrupt handler for audio
    #[task( binds = DMA1_STR1, resources = [audio, buffer, looper], priority = 8 )]
    fn audio_handler(ctx: audio_handler::Context) {
        let audio = ctx.resources.audio;
        let buffer = ctx.resources.buffer;
        let looper = ctx.resources.looper;

        if audio.get_stereo(buffer) {
            for (left, right) in buffer {
                let right = looper.process(*right);
                audio.push_stereo((*left, right)).unwrap();
            }
        } else {
            info!("Error reading data!");
        }
    }

    // Non-default idle ensures chip doesn't go to sleep which causes issues for
    // probe.rs currently
    #[idle]
    fn idle(_ctx: idle::Context) -> ! {
        loop {
            cortex_m::asm::nop();
        }
    }

    #[task( binds = TIM2, resources = [timer2, seed_led, switch1, looper] )]
    fn interface_handler(mut ctx: interface_handler::Context) {
        ctx.resources.timer2.clear_irq();
        let switch1 = ctx.resources.switch1;
        let seed_led = ctx.resources.seed_led;
        switch1.update();
        seed_led.update();

        // One button looper logic
        if switch1.is_held() {
            info!("Button held!");
            ctx.resources
                .looper
                .lock(|looper| match looper.get_state() {
                    LooperState::Stop => {
                        info!("Clear!");
                        looper.update(LooperState::Clear).unwrap();
                    }
                    LooperState::Play => {
                        info!("Undo!");
                        looper.update(LooperState::Undo).unwrap();
                    }
                    _ => (),
                });
        } else if switch1.is_double() {
            info!("Button pressed twice!");
            ctx.resources.looper.lock(|looper| {
                info!("Stop!");
                seed_led.clear_blink();
                seed_led.set_brightness(0.0);
                looper.update(LooperState::Stop).unwrap();
            });
        } else if switch1.is_falling() {
            ctx.resources
                .looper
                .lock(|looper| match looper.get_state() {
                    LooperState::Record => {
                        info!("Play!");
                        seed_led.clear_blink();
                        seed_led.set_brightness(1.0);
                        looper.update(LooperState::Play).unwrap()
                    }
                    LooperState::Play
                    | LooperState::Stop
                    | LooperState::Clear
                    | LooperState::Undo => {
                        info!("Record!");
                        seed_led.set_brightness(0.0);
                        seed_led.set_blink(0.1, 1.0);
                        looper.update(LooperState::Record).unwrap()
                    }
                });
        }
    }
};
