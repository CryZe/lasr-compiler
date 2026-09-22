use std::{pin::Pin, rc::Rc};

use asr::timer::{self, TimerState};
use tsuki::{Lua, Ref, Thread};

use crate::{state::State, utils::call_maybe};

pub struct TimerSnapshot {
    state: TimerState,
    split_index: Option<u64>,
}

impl TimerSnapshot {
    pub fn capture() -> Self {
        Self {
            state: timer::state(),
            split_index: timer::current_split_index(),
        }
    }

    pub async fn dispatch(
        self,
        next: &Self,
        lua: &Pin<Rc<Lua<State>>>,
        td: &Ref<'_, Thread<State>>,
    ) {
        if self.state == TimerState::NotRunning
            && matches!(next.state, TimerState::Running | TimerState::Paused)
        {
            call_maybe(lua, td, "onStart").await;
        }

        if let (Some(before), Some(after)) = (self.split_index, next.split_index) {
            if after > before {
                for index in before..after {
                    if timer::segment_splitted(index) == Some(false) {
                        call_maybe(lua, td, "onSkip").await;
                    } else {
                        call_maybe(lua, td, "onSplit").await;
                    }
                }
            } else if after < before {
                for _ in after..before {
                    call_maybe(lua, td, "onUnsplit").await;
                }
            }
        }

        if self.state != TimerState::NotRunning && next.state == TimerState::NotRunning {
            call_maybe(lua, td, "onReset").await;
        }
        if self.state == TimerState::Running && next.state == TimerState::Paused {
            call_maybe(lua, td, "onPause").await;
        } else if self.state == TimerState::Paused && next.state == TimerState::Running {
            call_maybe(lua, td, "onUnpause").await;
        }
    }
}
