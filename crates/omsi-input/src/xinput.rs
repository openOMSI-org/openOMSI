use std::ffi::c_void;
use windows::core::{s, w};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use crate::controllers::HAT_BUTTONS;

pub const MAX_SLOTS: usize = 4;

type GetStateFn = unsafe extern "system" fn(u32, *mut XInputState) -> u32;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GamepadState {
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub thumb_lx: i16,
    pub thumb_ly: i16,
    pub thumb_rx: i16,
    pub thumb_ry: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct XInputState {
    packet_number: u32,
    gamepad: GamepadState,
}

pub const BUTTON_MAP: [(u16, gilrs::Button, usize); 14] = [
    (0x1000, gilrs::Button::South, 0),
    (0x2000, gilrs::Button::East, 1),
    (0x4000, gilrs::Button::West, 2),
    (0x8000, gilrs::Button::North, 3),
    (0x0100, gilrs::Button::LeftTrigger, 4),
    (0x0200, gilrs::Button::RightTrigger, 5),
    (0x0020, gilrs::Button::Select, 6),
    (0x0010, gilrs::Button::Start, 7),
    (0x0040, gilrs::Button::LeftThumb, 8),
    (0x0080, gilrs::Button::RightThumb, 9),
    (0x0001, gilrs::Button::DPadUp, HAT_BUTTONS),
    (0x0008, gilrs::Button::DPadRight, HAT_BUTTONS + 1),
    (0x0002, gilrs::Button::DPadDown, HAT_BUTTONS + 2),
    (0x0004, gilrs::Button::DPadLeft, HAT_BUTTONS + 3),
];

pub const DEFAULT_INDICES: [usize; 14] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
    HAT_BUTTONS, HAT_BUTTONS + 1, HAT_BUTTONS + 2, HAT_BUTTONS + 3,
];

#[derive(Debug, Clone)]
struct Slot {
    connected: bool,
    state: GamepadState,
    name: Option<String>,
    indices: [usize; 14],
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            connected: false,
            state: GamepadState::default(),
            name: None,
            indices: DEFAULT_INDICES,
        }
    }
}

#[derive(Default)]
pub struct Reader {
    api: Option<Option<GetStateFn>>,
    slots: [Slot; MAX_SLOTS],
}

impl Reader {
    pub fn poll(
        &mut self,
        wgi_pads: &[(&str, [usize; 14])],
    ) -> Vec<(usize, String, Vec<(gilrs::Button, usize, bool)>)> {
        let Some(get_state) = self.load_api() else {
            return Vec::new();
        };
        self.poll_with(wgi_pads, |slot| read_xinput_slot(get_state, slot as u32))
    }

    pub fn clear_state(&mut self) {
        for slot in &mut self.slots {
            *slot = Slot::default();
        }
    }

    pub fn state_for_name(&self, name: &str) -> Option<GamepadState> {
        self.slots
            .iter()
            .find(|s| s.connected && s.name.as_deref() == Some(name))
            .map(|s| s.state)
    }

    fn load_api(&mut self) -> Option<GetStateFn> {
        *self.api.get_or_insert_with(bind_xinput)
    }

    fn poll_with(
        &mut self,
        wgi_pads: &[(&str, [usize; 14])],
        mut read_slot: impl FnMut(usize) -> Option<GamepadState>,
    ) -> Vec<(usize, String, Vec<(gilrs::Button, usize, bool)>)> {
        let mut raw = [None; MAX_SLOTS];
        for (idx, slot) in raw.iter_mut().enumerate() {
            *slot = read_slot(idx);
        }

        let mut updates = Vec::new();
        let mut pending = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        for (idx, slot) in self.slots.iter_mut().enumerate() {
            match raw[idx] {
                Some(curr) => {
                    let prev_buttons = if slot.connected { slot.state.buttons } else { 0 };
                    pending[idx] = button_edges(prev_buttons, curr.buttons, &slot.indices).collect();
                    slot.connected = true;
                    slot.state = curr;
                }
                None if slot.connected => {
                    let edges: Vec<_> = button_edges(slot.state.buttons, 0, &slot.indices).collect();
                    if let Some(name) = slot.name.take() {
                        if !edges.is_empty() {
                            updates.push((idx, name, edges));
                        }
                    }
                    *slot = Slot::default();
                }
                None => {
                    slot.name = None;
                }
            }
        }

        let mut available: Vec<bool> = vec![true; wgi_pads.len()];
        let live_names: Vec<Option<String>> = self.slots.iter().map(|s| s.connected.then(|| s.name.clone()).flatten()).collect();
        for (idx, slot) in self.slots.iter_mut().enumerate() {
            if !slot.connected {
                slot.name = None;
                continue;
            }
            if let Some(existing) = live_names[idx].as_deref() {
                let live_named = live_names.iter().filter(|n| n.as_deref() == Some(existing)).count();
                let wgi_named = wgi_pads.iter().filter(|(n, _)| *n == existing).count();
                if live_named <= wgi_named {
                    if let Some(pos) = wgi_pads
                        .iter()
                        .enumerate()
                        .find(|(i, (n, _))| available[*i] && *n == existing)
                        .map(|(i, _)| i)
                    {
                        available[pos] = false;
                        slot.indices = wgi_pads[pos].1;
                        continue;
                    }
                }
                slot.name = None;
            }
        }

        // Pair only when one unclaimed XInput slot and one unclaimed WGI pad match unambiguously.
        let unclaimed_slots: Vec<usize> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (s.connected && s.name.is_none()).then_some(i))
            .collect();
        let unclaimed_wgi: Vec<usize> = available
            .iter()
            .enumerate()
            .filter_map(|(i, free)| free.then_some(i))
            .collect();
        if let ([slot_idx], [wgi_pos]) = (unclaimed_slots.as_slice(), unclaimed_wgi.as_slice()) {
            let slot = &mut self.slots[*slot_idx];
            slot.name = Some(wgi_pads[*wgi_pos].0.to_string());
            slot.indices = wgi_pads[*wgi_pos].1;
            if let Some(curr) = raw[*slot_idx] {
                pending[*slot_idx] = button_edges(0, curr.buttons, &slot.indices).collect();
            }
        }

        for (idx, edges) in pending.into_iter().enumerate() {
            if !edges.is_empty() {
                if let Some(name) = self.slots[idx].name.as_ref() {
                    updates.push((idx, name.clone(), edges));
                }
            }
        }
        updates
    }
}

pub fn normalize_thumb(v: i16) -> f32 {
    if v < 0 { v as f32 / 32768.0 } else { v as f32 / 32767.0 }
}

pub fn normalize_trigger(v: u8) -> f32 {
    v as f32 / 255.0
}

pub fn button_edges<'a>(
    prev: u16,
    curr: u16,
    indices: &'a [usize; 14],
) -> impl Iterator<Item = (gilrs::Button, usize, bool)> + 'a {
    let changed = prev ^ curr;
    BUTTON_MAP.iter().zip(indices.iter()).filter_map(move |(&(mask, btn, _), &idx)| {
        (changed & mask != 0).then_some((btn, idx, curr & mask != 0))
    })
}

fn bind_xinput() -> Option<GetStateFn> {
    for dll_name in [w!("xinput1_4.dll"), w!("xinput9_1_0.dll")] {
        let Ok(module): Result<HMODULE, _> = (unsafe { LoadLibraryW(dll_name) }) else {
            continue;
        };
        if let Some(proc) = unsafe { GetProcAddress(module, s!("XInputGetState")) } {
            let fn_ptr = unsafe { std::mem::transmute::<*const c_void, GetStateFn>(proc as *const c_void) };
            return Some(fn_ptr);
        }
    }
    None
}

fn read_xinput_slot(get_state: GetStateFn, user_index: u32) -> Option<GamepadState> {
    let mut state = XInputState::default();
    let status = unsafe { get_state(user_index, &mut state) };
    (status == 0).then_some(state.gamepad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_with_emits_button_edges_pairs_wgi_name_and_releases_on_disconnect() {
        let mut r = Reader::default();
        let mut custom = DEFAULT_INDICES;
        custom[1] = 7;
        let pads = [("Xbox One Game Controller", custom)];
        let mut s0 = Some(GamepadState {
            buttons: 0x1001,
            left_trigger: 255,
            thumb_lx: -32768,
            ..Default::default()
        });
        let u1 = r.poll_with(&pads, |i| if i == 1 { s0 } else { None });
        assert_eq!(u1.len(), 1);
        assert_eq!(u1[0].0, 1);
        assert_eq!(u1[0].1, "Xbox One Game Controller");
        assert!(u1[0].2.contains(&(gilrs::Button::South, 0, true)));
        assert!(u1[0].2.contains(&(gilrs::Button::DPadUp, HAT_BUTTONS, true)));
        assert_eq!(r.state_for_name("Xbox One Game Controller"), s0);

        s0 = Some(GamepadState { buttons: 0x2000, ..Default::default() });
        let u2 = r.poll_with(&pads, |i| if i == 1 { s0 } else { None });
        assert!(u2[0].2.contains(&(gilrs::Button::South, 0, false)));
        assert!(u2[0].2.contains(&(gilrs::Button::DPadUp, HAT_BUTTONS, false)));
        assert!(u2[0].2.contains(&(gilrs::Button::East, 7, true)));

        let u3 = r.poll_with(&[], |_| None);
        assert_eq!(u3, vec![(1, "Xbox One Game Controller".into(), vec![(gilrs::Button::East, 7, false)])]);
        assert_eq!(r.state_for_name("Xbox One Game Controller"), None);
    }

    #[test]
    fn ambiguous_two_slots_and_two_pads_stay_unnamed_and_mismatch_count_stays_unpaired() {
        let mut r = Reader::default();
        let pads = [
            ("Xbox One Game Controller", DEFAULT_INDICES),
            ("Flydigi Direwolf 4", DEFAULT_INDICES),
        ];
        let active = Some(GamepadState { buttons: 0x1000, ..Default::default() });
        let updates = r.poll_with(&pads, |i| if i < 2 { active } else { None });
        assert!(updates.is_empty());
        assert_eq!(r.state_for_name("Xbox One Game Controller"), None);
        assert_eq!(r.state_for_name("Flydigi Direwolf 4"), None);

        let updates_single = r.poll_with(&pads[..1], |i| if i < 2 { active } else { None });
        assert!(updates_single.is_empty());
        assert_eq!(r.state_for_name("Xbox One Game Controller"), None);
    }

    #[test]
    fn thumb_and_trigger_normalization_covers_full_range() {
        assert_eq!(normalize_thumb(0), 0.0);
        assert_eq!(normalize_thumb(i16::MIN), -1.0);
        assert_eq!(normalize_thumb(i16::MAX), 1.0);
        assert_eq!(normalize_trigger(0), 0.0);
        assert_eq!(normalize_trigger(255), 1.0);
    }
}
