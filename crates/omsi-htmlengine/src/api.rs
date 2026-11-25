//! The vehicle snapshot (see [`omsi_sim::vehicle_api`]) as script values.

use super::*;
use omsi_sim::vehicle_api::ApiValue;

/// The script object tree for a snapshot: maps become objects, lists arrays.
pub(crate) fn api_to_val(a: &ApiValue) -> Val {
    match a {
        ApiValue::Null => Val::Null,
        ApiValue::Bool(b) => Val::Bool(*b),
        ApiValue::Num(n) => Val::Num(*n),
        ApiValue::Str(s) => Val::Str(s.clone()),
        ApiValue::List(l) => Val::Arr(Arc::new(Mutex::new(l.iter().map(api_to_val).collect()))),
        ApiValue::Map(m) => Val::Obj(Arc::new(Mutex::new(
            m.iter().map(|(k, v)| (k.clone(), api_to_val(v))).collect(),
        ))),
    }
}
