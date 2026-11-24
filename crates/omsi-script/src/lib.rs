//! OMSI object scripts (`.osc`) - a re-implementation of the original `mc_exprcalc` unit.
//!
//! See `docs/FORMATS.md` for the language rules. A [`Program`] is compiled once per object
//! *type* (vehicle, scenery object, script texture …) from its varlists, stringvarlists,
//! constfiles and script files; every object *instance* owns a [`State`] (its variables) and is
//! executed by [`Vm`] against a [`Host`] that supplies system variables, callbacks and sound
//! triggers.

pub mod compat;
pub mod compile;
pub mod constfile;
pub mod sysvar;
pub mod vm;

pub use compile::{CompileInput, Program, ScriptError, compile};
pub use constfile::{ConstFile, Curve};
pub use sysvar::SysVar;
pub use vm::{Host, NullHost, Stacks, State, Vm, mix_seed, session_seed, set_session_seed};

/// Identifier of a float variable inside a [`Program`].
pub type VarId = u32;
/// Identifier of a string variable inside a [`Program`].
pub type StrVarId = u32;
/// Identifier of an interned name (callback, sound trigger, map variable).
pub type NameId = u32;
/// Index of a compiled block.
pub type BlockId = u32;

/// One instruction. Every arithmetic instruction *pushes* its result (OMSI semantics).
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Push(f32),
    Load(VarId),
    Store(VarId),
    LoadSys(SysVar),
    StoreSys(SysVar),
    Const(f32),
    Curve(u32),
    Macro(BlockId),
    Callback(NameId),
    SoundTrigger(NameId),
    /// `(T.F.name)`: a sound trigger with the file to play taken from the string stack
    /// (station announcements).
    SoundTriggerFile(NameId),
    LoadStr(StrVarId),
    StoreStr(StrVarId),
    PushStr(u32),
    LoadReg(u8),
    StoreReg(u8),
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    Min,
    Max,
    Not,
    Neg,
    Dup,
    Sin,
    ArcSin,
    ArcTan,
    Exp,
    Sqrt,
    Sqr,
    Sgn,
    Abs,
    Trunc,
    Pi,
    Random,
    StackDump,
    /// Jump to the given instruction index if `st[0] == 0`.
    JumpIfZero(u32),
    Jump(u32),
    StrConcat,
    StrEq,
    StrLt,
    StrGt,
    StrLe,
    StrGe,
    StrDup,
    StrRepeat,
    StrLength,
    StrMsg,
    StrRemoveSpaces,
    StrCutBegin,
    StrCutEnd,
    StrSetLengthL,
    StrSetLengthR,
    StrSetLengthC,
    StrIntToStr,
    StrIntToStrEnh,
    StrToFloat,
    /// Not an OMSI operator: [`compat`] puts it into a line display's script (a number
    /// display of three digits and a letter) as `$__DigitsFirst`.
    StrDigitsFirst,
}

const _: () = assert!(std::mem::size_of::<Op>() == 8);
