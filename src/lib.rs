#![no_std]
pub type Buffer<const N: usize> = [f32; N];

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LooperState {
    Record,
    Play,
    Undo,
    Stop,
    Clear,
}

#[derive(Debug)]
pub enum LooperError {
    InvalidStateTransition,
}

///Looper struct
pub struct Looper<const N: usize> {
    buf1: Option<&'static mut Buffer<N>>,
    buf2: Option<&'static mut Buffer<N>>,
    state: LooperState,
    index: usize,
    tail: usize,
    is_looping: bool,
}

impl<const N: usize> Looper<N> {
    /// A single buffer enables everything but undo capability if space is a concern
    /// Passing in a 2nd buffer enables undo
    pub fn new(buf1: &'static mut Buffer<N>, buf2: Option<&'static mut Buffer<N>>) -> Self {
        if let Some(buf2) = &buf2 {
            if buf1.len() != buf2.len() {
                panic!("Buffers must be the same size!");
            }
        };
        let tail = buf1.len();
        Self {
            buf1: Some(buf1),
            buf2,
            state: LooperState::Clear,
            index: 0,
            tail,
            is_looping: false,
        }
    }

    pub fn get_state(&self) -> LooperState {
        self.state
    }

    /// Call this once per sample to handle data
    /// Input is added to recorded data
    pub fn process(&mut self, input: f32) -> f32 {
        let mut output = input;
        // Buf1 must exist, unwrap a mutable
        let buf1 = self.buf1.as_mut().unwrap();

        if self.state == LooperState::Record {
            if !self.is_looping {
                // Recording to "empty" buffers, overwrite data
                buf1[self.index] = input;
                // If we have an undo buffer overwrite it too
                if let Some(buf2) = &mut self.buf2 {
                    buf2[self.index] = input;
                }
            } else {
                // Overdub
                buf1[self.index] += input;
            }
        }

        // If state is Play or Record, add the current loop sample to playback and update the index
        if self.state == LooperState::Play
            || self.state == LooperState::Record
            || self.state == LooperState::Undo
        {
            if self.is_looping {
                output += buf1[self.index];
            }
            self.index = (self.index + 1) % self.tail;
        }
        output
    }

    /// Update the state of the looper, consumes and returns self
    pub fn update(&mut self, new_state: LooperState) -> Result<(), LooperError> {
        match new_state {
            LooperState::Record => {
                // If the requested state is record and the current state
                // is stop, clear, or play, enter recording state
                if self.state == LooperState::Stop
                    || self.state == LooperState::Clear
                    || self.state == LooperState::Play
                    || self.state == LooperState::Undo
                {
                    if self.state != LooperState::Undo {
                        self.swap_buf();
                    }
                    self.state = LooperState::Record;
                    Ok(())
                } else {
                    Err(LooperError::InvalidStateTransition)
                }
            }
            LooperState::Play => {
                // If the requested state is play and the current state is recording enter
                // playback state
                if self.state == LooperState::Record {
                    self.state = LooperState::Play;
                    // Latch the loop length to the current length
                    if !self.is_looping {
                        self.is_looping = true;
                        // The end is one minus the current index, allowing for 0 which means it points to the end
                        if self.index > 0 {
                            self.tail = self.index - 1;
                        }
                    }
                    Ok(())
                } else {
                    Err(LooperError::InvalidStateTransition)
                }
            }
            LooperState::Undo => {
                if self.state == LooperState::Play {
                    self.state = LooperState::Undo;
                }
                self.swap_buf();
                Ok(())
            }
            LooperState::Stop => {
                self.state = LooperState::Stop;
                Ok(())
            }
            LooperState::Clear => {
                self.index = 0;
                self.tail = self.buf1.as_ref().unwrap().len();
                self.is_looping = false;
                self.state = LooperState::Clear;
                Ok(())
            }
        }
    }

    /// Swaps the current/undo buffers
    /// Users should call update(LooperState::Undo)
    fn swap_buf(&mut self) {
        if let Some(buf2) = self.buf2.take() {
            self.buf2 = Some(self.buf1.take().unwrap());
            self.buf1 = Some(buf2);
        }
    }
}
