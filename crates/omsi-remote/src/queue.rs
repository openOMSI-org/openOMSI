use crate::Command;
use std::collections::VecDeque;
use std::mem::discriminant;

pub(crate) const MAX_QUEUED: usize = 64;
pub(crate) const PER_FRAME: usize = 24;

#[derive(Default)]
pub(crate) struct Queue {
    commands: VecDeque<Command>,
}

fn release(c: &Command) -> bool {
    matches!(c, Command::Action { down: false, .. })
}

fn state_like(c: &Command) -> bool {
    matches!(c, Command::Indicator(_) | Command::Route(_) | Command::Line(_) | Command::Destination(_) | Command::ClearLine | Command::NextStop(_))
}

impl Queue {
    pub(crate) fn push(&mut self, c: Command) -> bool {
        if state_like(&c) {
            if let Some(old) = self.commands.back_mut().filter(|o| discriminant(*o) == discriminant(&c)) {
                *old = c;
                return true;
            }
        }
        if !release(&c) && self.commands.iter().filter(|o| !release(o)).count() >= MAX_QUEUED {
            return false;
        }
        self.commands.push_back(c);
        true
    }

    pub(crate) fn take(&mut self, most: usize) -> Vec<Command> {
        let n = most.min(self.commands.len());
        self.commands.drain(..n).collect()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.commands.len()
    }
}
