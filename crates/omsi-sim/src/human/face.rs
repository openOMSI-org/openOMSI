//! Faces that live: the blend shapes of an imported avatar (the Rocketbox avatars carry the
//! 52 ARKit shapes and 15 visemes) driven by a small model of what a face does on a bus.
//!
//! - The eyes blink every few seconds (more often while talking), now and then twice, and
//!   rest on a point that changes with quick saccades; they lead where the head turns.
//! - Everybody has a mood of their own: some sit with a faint smile, some with a set mouth,
//!   and once in a while a face does something - a smile, raised brows, a frown of
//!   thought, pressed lips, a yawn late in the evening, a squint in bright sun.
//! - Paying the driver is done with a smile and a look at the driver.
//! - A passenger who says something moves the lips with what is said: the jaw opens with
//!   the loudness of the recording, and the mouth goes through the viseme shapes at the
//!   pace of the syllables.
//!
//! Nothing here is the simulation's: it is the look of a person as drawn, made per person
//! from its own seed.

/// The expressions a face is made of, by the ARKit / viseme names of the blend shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Channel {
    BlinkL,
    BlinkR,
    LookUpL,
    LookUpR,
    LookDownL,
    LookDownR,
    LookInL,
    LookInR,
    LookOutL,
    LookOutR,
    SquintL,
    SquintR,
    WideL,
    WideR,
    BrowDownL,
    BrowDownR,
    BrowInnerUp,
    BrowOuterUpL,
    BrowOuterUpR,
    CheekSquintL,
    CheekSquintR,
    SmileL,
    SmileR,
    FrownL,
    FrownR,
    PressL,
    PressR,
    MouthClose,
    JawOpen,
    Pucker,
    Funnel,
    NoseSneerL,
    NoseSneerR,
    VisemePP,
    VisemeFF,
    VisemeTH,
    VisemeDD,
    VisemeKK,
    VisemeCH,
    VisemeSS,
    VisemeNN,
    VisemeRR,
    VisemeAA,
    VisemeE,
    VisemeI,
    VisemeO,
    VisemeU,
}

pub const CHANNELS: usize = Channel::VisemeU as usize + 1;

/// The blend shape names each channel goes by (lower case, without the numbering prefix the
/// Rocketbox files give them: `AK_09_EyeBlinkLeft` is `eyeblinkleft`).
const NAMES: [&str; CHANNELS] = [
    "eyeblinkleft",
    "eyeblinkright",
    "eyelookupleft",
    "eyelookupright",
    "eyelookdownleft",
    "eyelookdownright",
    "eyelookinleft",
    "eyelookinright",
    "eyelookoutleft",
    "eyelookoutright",
    "eyesquintleft",
    "eyesquintright",
    "eyewideleft",
    "eyewideright",
    "browdownleft",
    "browdownright",
    "browinnerup",
    "browouterupleft",
    "browouterupright",
    "cheeksquintleft",
    "cheeksquintright",
    "mouthsmileleft",
    "mouthsmileright",
    "mouthfrownleft",
    "mouthfrownright",
    "mouthpressleft",
    "mouthpressright",
    "mouthclose",
    "jawopen",
    "mouthpucker",
    "mouthfunnel",
    "nosesneerleft",
    "nosesneerright",
    "pp",
    "ff",
    "th",
    "dd",
    "kk",
    "ch",
    "ss",
    "nn",
    "rr",
    "aa",
    "e",
    "i",
    "o",
    "u",
];

/// A blend shape's name as the channels know it: lower case, the numbering (`AK_09_`,
/// `AA_VI_10_`) and any namespace (`blendShape1.`) left out.
pub fn shape_key(name: &str) -> String {
    let n = name.rsplit(['.', ':', '|']).next().unwrap_or(name);
    let parts: Vec<&str> = n.split('_').collect();
    // the last part that is not a number
    let tail = parts
        .iter()
        .rev()
        .find(|p| !p.is_empty() && !p.chars().all(|c| c.is_ascii_digit()))
        .copied()
        .unwrap_or(n);
    let key = tail.to_ascii_lowercase();
    // (visemes: `AA_VI_00_Sil`, `AA_VI_10_aa`; ARKit: `AK_25_JawOpen`)
    key
}

/// Which blend shape of the mesh each channel moves.
#[derive(Debug, Clone)]
pub struct FaceRig {
    pub shape: [Option<u16>; CHANNELS],
}

impl Default for FaceRig {
    fn default() -> Self {
        FaceRig {
            shape: [None; CHANNELS],
        }
    }
}

impl FaceRig {
    /// The rig of a mesh with these blend shapes; None when it has too few to make a face
    /// (no blinks and no jaw).
    pub fn from_shapes<'a>(names: impl Iterator<Item = &'a str>) -> Option<FaceRig> {
        let mut rig = FaceRig::default();
        for (k, n) in names.enumerate() {
            let key = shape_key(n);
            // (a viseme and an ARKit shape may share the key of a short one: "e", "i" are visemes only)
            let viseme = n.contains("VI_") || n.to_ascii_lowercase().starts_with("viseme");
            if let Some(c) = NAMES.iter().position(|x| *x == key) {
                let is_viseme_channel = c >= Channel::VisemePP as usize;
                if is_viseme_channel == viseme && rig.shape[c].is_none() {
                    rig.shape[c] = Some(k as u16);
                }
            }
        }
        (rig.shape[Channel::BlinkL as usize].is_some()
            || rig.shape[Channel::JawOpen as usize].is_some())
        .then_some(rig)
    }

    /// The indices of the shapes the face uses.
    pub fn used(&self) -> impl Iterator<Item = u16> + '_ {
        self.shape.iter().flatten().copied()
    }
}

/// What the face reacts to this frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct FaceInput {
    /// Paying at the desk (a smile and a look at the driver).
    pub paying: bool,
    /// Walking (the face of somebody on the way).
    pub walking: bool,
    /// The loudness of what the person says now (0 … 1), if they talk.
    pub talking: Option<f32>,
    /// Hour of the day (yawns late at night and early in the morning).
    pub hour: f32,
    /// Bright sun in the face (0 … 1): a squint.
    pub glare: f32,
    /// Where the head looks relative to the body (degrees, right and up): the eyes lead it.
    pub head_turn: (f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Expression {
    Smile,
    Brows,
    Think,
    Press,
    Yawn,
    Pucker,
}

/// One face's state (per person as drawn).
#[derive(Debug, Clone)]
pub struct Face {
    rng: u64,
    time: f32,
    next_blink: f32,
    blink_t: f32,
    double_blink: bool,
    gaze: (f32, f32),
    gaze_target: (f32, f32),
    next_saccade: f32,
    /// The person's own resting face: a smile (-1 a set, unhappy mouth … 1 a smile).
    mood: f32,
    expression: Option<(Expression, f32, f32, f32)>,
    next_expression: f32,
    /// Talking: the viseme shown and the next, the blend between them, the open jaw.
    viseme: (usize, usize, f32),
    syllable: f32,
    jaw: f32,
    talk_amount: f32,
    pay_smile: f32,
    weights: [f32; CHANNELS],
}

const VISEMES: [Channel; 14] = [
    Channel::VisemeAA,
    Channel::VisemeE,
    Channel::VisemeO,
    Channel::VisemeI,
    Channel::VisemeU,
    Channel::VisemePP,
    Channel::VisemeFF,
    Channel::VisemeDD,
    Channel::VisemeKK,
    Channel::VisemeSS,
    Channel::VisemeNN,
    Channel::VisemeRR,
    Channel::VisemeCH,
    Channel::VisemeTH,
];

impl Face {
    pub fn new(seed: u64) -> Face {
        let mut f = Face {
            rng: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            time: 0.0,
            next_blink: 0.0,
            blink_t: -1.0,
            double_blink: false,
            gaze: (0.0, 0.0),
            gaze_target: (0.0, 0.0),
            next_saccade: 0.0,
            mood: 0.0,
            expression: None,
            next_expression: 0.0,
            viseme: (0, 0, 0.0),
            syllable: 0.0,
            jaw: 0.0,
            talk_amount: 0.0,
            pay_smile: 0.0,
            weights: [0.0; CHANNELS],
        };
        // a resting face of one's own: most neutral, some a little friendly, some set
        let m = f.rand();
        f.mood = if m < 0.3 {
            0.15 + 0.25 * f.rand()
        } else if m < 0.45 {
            -0.15 - 0.2 * f.rand()
        } else {
            0.05 * (f.rand() - 0.5)
        };
        f.next_blink = 0.5 + 3.0 * f.rand();
        f.next_saccade = 0.3 + f.rand();
        f.next_expression = 4.0 + 20.0 * f.rand();
        f
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        ((x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / (1u64 << 24) as f32
    }

    /// Advance by `dt` seconds; the weights of the channels (0 … 1).
    pub fn advance(&mut self, input: &FaceInput, dt: f32) -> &[f32; CHANNELS] {
        let dt = dt.clamp(0.0, 0.25);
        self.time += dt;
        let mut w = [0.0f32; CHANNELS];
        let ease = |x: f32| x * x * (3.0 - 2.0 * x);
        // --- blinks: ~150 ms shut and open, every 2-6 s (1.5-4 s talking), sometimes twice
        if self.blink_t < 0.0 && self.time >= self.next_blink {
            self.blink_t = 0.0;
            self.double_blink = self.rand() < 0.12;
            let talking = input.talking.is_some();
            self.next_blink = self.time
                + if talking {
                    1.5 + 2.5 * self.rand()
                } else {
                    2.0 + 4.5 * self.rand()
                };
        }
        let mut blink = 0.0;
        if self.blink_t >= 0.0 {
            self.blink_t += dt;
            let t = self.blink_t;
            // shut quickly (60 ms), stay 30 ms, open more slowly (110 ms)
            blink = if t < 0.06 {
                ease(t / 0.06)
            } else if t < 0.09 {
                1.0
            } else if t < 0.2 {
                1.0 - ease((t - 0.09) / 0.11)
            } else {
                0.0
            };
            if t >= 0.2 {
                if self.double_blink {
                    self.double_blink = false;
                    self.blink_t = 0.0;
                } else {
                    self.blink_t = -1.0;
                }
            }
        }
        // --- gaze: saccades to a new point every 0.4-3 s, the eyes leading the head
        if self.time >= self.next_saccade {
            let (hx, hy) = input.head_turn;
            let lead = (
                hx.clamp(-40.0, 40.0) / 40.0 * 0.5,
                hy.clamp(-30.0, 30.0) / 30.0 * 0.4,
            );
            self.gaze_target = (
                lead.0 + 0.5 * (self.rand() - 0.5),
                lead.1 + 0.35 * (self.rand() - 0.55),
            );
            if input.paying {
                self.gaze_target = (0.0, 0.05);
            }
            self.next_saccade = self.time + 0.4 + 2.6 * self.rand() * self.rand();
        }
        // (a saccade takes ~40 ms)
        let k = (dt / 0.04).min(1.0);
        self.gaze.0 += (self.gaze_target.0 - self.gaze.0) * k;
        self.gaze.1 += (self.gaze_target.1 - self.gaze.1) * k;
        let (gx, gy) = self.gaze;
        // looking right: the right eye looks out, the left one in
        w[Channel::LookOutR as usize] = gx.max(0.0);
        w[Channel::LookInL as usize] = gx.max(0.0);
        w[Channel::LookOutL as usize] = (-gx).max(0.0);
        w[Channel::LookInR as usize] = (-gx).max(0.0);
        w[Channel::LookUpL as usize] = gy.max(0.0);
        w[Channel::LookUpR as usize] = gy.max(0.0);
        w[Channel::LookDownL as usize] = (-gy).max(0.0);
        w[Channel::LookDownR as usize] = (-gy).max(0.0);
        // the upper lids follow a look down
        let lid = 0.35 * (-gy).max(0.0);
        w[Channel::BlinkL as usize] = (blink + lid).min(1.0);
        w[Channel::BlinkR as usize] = (blink + lid).min(1.0);
        // --- the resting face
        if self.mood > 0.0 {
            w[Channel::SmileL as usize] += self.mood;
            w[Channel::SmileR as usize] += self.mood * 0.9;
        } else {
            w[Channel::FrownL as usize] += -self.mood;
            w[Channel::FrownR as usize] += -self.mood;
            w[Channel::PressL as usize] += -self.mood * 0.5;
            w[Channel::PressR as usize] += -self.mood * 0.5;
        }
        // --- now and then an expression, eased in and out
        if self.expression.is_none() && self.time >= self.next_expression && input.talking.is_none()
        {
            let r = self.rand();
            let late = input.hour < 6.5 || input.hour > 22.0;
            let e = if late && r < 0.25 {
                Expression::Yawn
            } else if r < 0.35 {
                Expression::Smile
            } else if r < 0.5 {
                Expression::Brows
            } else if r < 0.7 {
                Expression::Think
            } else if r < 0.88 {
                Expression::Press
            } else {
                Expression::Pucker
            };
            let len = match e {
                Expression::Yawn => 3.5,
                Expression::Brows => 0.9 + 0.6 * self.rand(),
                _ => 1.5 + 3.0 * self.rand(),
            };
            let amount = 0.35 + 0.5 * self.rand();
            self.expression = Some((e, self.time, len, amount));
        }
        if let Some((e, start, len, amount)) = self.expression {
            let t = self.time - start;
            if t >= len {
                self.expression = None;
                self.next_expression = self.time + 6.0 + 22.0 * self.rand();
            } else {
                let fade = (t / 0.35).min(1.0).min((len - t) / 0.45).max(0.0);
                let a = ease(fade) * amount;
                match e {
                    Expression::Smile => {
                        w[Channel::SmileL as usize] += a;
                        w[Channel::SmileR as usize] += a;
                        w[Channel::CheekSquintL as usize] += 0.5 * a;
                        w[Channel::CheekSquintR as usize] += 0.5 * a;
                        w[Channel::SquintL as usize] += 0.3 * a;
                        w[Channel::SquintR as usize] += 0.3 * a;
                    }
                    Expression::Brows => {
                        w[Channel::BrowInnerUp as usize] += a;
                        w[Channel::BrowOuterUpL as usize] += 0.8 * a;
                        w[Channel::BrowOuterUpR as usize] += 0.8 * a;
                        w[Channel::WideL as usize] += 0.3 * a;
                        w[Channel::WideR as usize] += 0.3 * a;
                    }
                    Expression::Think => {
                        w[Channel::BrowDownL as usize] += 0.6 * a;
                        w[Channel::BrowDownR as usize] += 0.4 * a;
                        w[Channel::PressL as usize] += 0.3 * a;
                        w[Channel::PressR as usize] += 0.3 * a;
                        w[Channel::SquintL as usize] += 0.2 * a;
                    }
                    Expression::Press => {
                        w[Channel::PressL as usize] += 0.6 * a;
                        w[Channel::PressR as usize] += 0.6 * a;
                        w[Channel::MouthClose as usize] += 0.2 * a;
                    }
                    Expression::Pucker => {
                        w[Channel::Pucker as usize] += 0.5 * a;
                        w[Channel::BrowInnerUp as usize] += 0.2 * a;
                    }
                    Expression::Yawn => {
                        // the mouth opens wide for most of it, the eyes shut
                        let open = ease(((t - 0.4) / 1.0).clamp(0.0, 1.0))
                            * ease(((len - t) / 0.9).clamp(0.0, 1.0));
                        w[Channel::JawOpen as usize] += 0.85 * open;
                        w[Channel::Funnel as usize] += 0.2 * open;
                        w[Channel::BrowInnerUp as usize] += 0.4 * open;
                        w[Channel::SquintL as usize] += 0.7 * open;
                        w[Channel::SquintR as usize] += 0.7 * open;
                        w[Channel::BlinkL as usize] =
                            (w[Channel::BlinkL as usize] + 0.6 * open).min(1.0);
                        w[Channel::BlinkR as usize] =
                            (w[Channel::BlinkR as usize] + 0.6 * open).min(1.0);
                    }
                }
            }
        }
        // --- paying: a smile to the driver
        let pay_to = if input.paying { 0.55 } else { 0.0 };
        self.pay_smile += (pay_to - self.pay_smile) * (dt / 0.6).min(1.0);
        w[Channel::SmileL as usize] += self.pay_smile;
        w[Channel::SmileR as usize] += self.pay_smile;
        w[Channel::CheekSquintL as usize] += 0.4 * self.pay_smile;
        w[Channel::CheekSquintR as usize] += 0.4 * self.pay_smile;
        // --- glare: a squint
        if input.glare > 0.0 {
            let g = input.glare.clamp(0.0, 1.0);
            w[Channel::SquintL as usize] += 0.6 * g;
            w[Channel::SquintR as usize] += 0.6 * g;
            w[Channel::BrowDownL as usize] += 0.3 * g;
            w[Channel::BrowDownR as usize] += 0.3 * g;
        }
        // --- talking: the jaw with the loudness, the visemes at the syllables' pace
        let loud = input.talking.unwrap_or(0.0).clamp(0.0, 1.0);
        self.talk_amount +=
            ((input.talking.is_some() as u8 as f32) - self.talk_amount) * (dt / 0.12).min(1.0);
        if input.talking.is_some() {
            // ~5 syllables a second, a little irregular
            self.syllable += dt * (4.0 + 2.5 * self.rand());
            if self.syllable >= 1.0 {
                self.syllable -= 1.0;
                self.viseme.0 = self.viseme.1;
                // open vowels most of the time, consonants between
                let r = self.rand();
                self.viseme.1 = if r < 0.55 {
                    (self.rand() * 5.0) as usize
                } else {
                    5 + (self.rand() * 9.0) as usize
                };
            }
            self.viseme.2 = ease(self.syllable);
        }
        let jaw_to = (loud * 1.6).min(0.55) * self.talk_amount;
        self.jaw += (jaw_to - self.jaw) * (dt / 0.05).min(1.0);
        if self.talk_amount > 0.01 {
            let (a, b, t) = self.viseme;
            let a_ch = VISEMES[a.min(VISEMES.len() - 1)] as usize;
            let b_ch = VISEMES[b.min(VISEMES.len() - 1)] as usize;
            let level = self.talk_amount * (0.4 + 0.6 * (loud * 2.0).min(1.0));
            w[a_ch] += (1.0 - t) * level;
            w[b_ch] += t * level;
            w[Channel::JawOpen as usize] += self.jaw;
            // stressed syllables raise the brows a little
            w[Channel::BrowInnerUp as usize] += 0.25 * (loud - 0.5).max(0.0) * self.talk_amount;
        }
        if input.walking {
            // (lips a little apart, breathing)
            w[Channel::JawOpen as usize] += 0.04;
        }
        for (o, v) in self.weights.iter_mut().zip(w) {
            *o = v.clamp(0.0, 1.0);
        }
        &self.weights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rocketbox_shape_names_are_found() {
        let names = [
            "blendShape1.AA_VI_00_Sil",
            "blendShape1.AA_VI_10_aa",
            "AA_VI_11_E",
            "AK_09_EyeBlinkLeft",
            "AK_10_EyeBlinkRight",
            "AK_25_JawOpen",
            "AK_44_MouthSmileLeft",
            "AU_45_Blink",
        ];
        let rig = FaceRig::from_shapes(names.iter().copied()).unwrap();
        assert_eq!(rig.shape[Channel::VisemeAA as usize], Some(1));
        assert_eq!(rig.shape[Channel::VisemeE as usize], Some(2));
        assert_eq!(rig.shape[Channel::BlinkL as usize], Some(3));
        assert_eq!(rig.shape[Channel::JawOpen as usize], Some(5));
        assert_eq!(rig.shape[Channel::SmileL as usize], Some(6));
        assert!(FaceRig::from_shapes(["Smile"].iter().copied()).is_none());
    }

    #[test]
    fn a_face_blinks_and_moves_its_lips_when_talking() {
        let mut f = Face::new(7);
        let mut blinked = false;
        for _ in 0..(10.0 / 0.016) as usize {
            let w = f.advance(
                &FaceInput {
                    hour: 12.0,
                    ..Default::default()
                },
                0.016,
            );
            blinked |= w[Channel::BlinkL as usize] > 0.95;
            assert!(w.iter().all(|v| (0.0..=1.0).contains(v)));
        }
        assert!(blinked, "no blink in ten seconds");
        let mut jaw = 0.0f32;
        for k in 0..120 {
            let loud = if k % 10 < 5 { 0.8 } else { 0.1 };
            let w = f.advance(
                &FaceInput {
                    talking: Some(loud),
                    hour: 12.0,
                    ..Default::default()
                },
                0.016,
            );
            jaw = jaw.max(w[Channel::JawOpen as usize]);
        }
        assert!(jaw > 0.3, "the jaw stayed shut while talking: {jaw}");
    }
}
