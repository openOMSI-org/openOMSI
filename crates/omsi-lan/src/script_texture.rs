//! Script texture identity and compatibility for peer subscriptions.
use crate::sync::{VarTable, var_table};

/// Bus path and declared surfaces must agree; the seed is the owner's variable table.
pub fn script_texture_table(
    bus: &str,
    program: &omsi_script::Program,
    surfaces: &[(i32, i32)],
) -> u32 {
    script_texture_table_for_vars(bus, var_table(program).hash, surfaces)
}

pub(crate) fn script_texture_table_for_vars(bus: &str, mut hash: u32, surfaces: &[(i32, i32)]) -> u32 {
    for b in bus
        .replace('\\', "/")
        .bytes()
        .map(|b| b.to_ascii_lowercase())
        .chain(std::iter::once(0))
        .chain((surfaces.len() as u32).to_le_bytes())
        .chain(
            surfaces
                .iter()
                .flat_map(|(w, h)| w.to_le_bytes().into_iter().chain(h.to_le_bytes())),
        )
    {
        hash = (hash ^ b as u32).wrapping_mul(0x0100_0193);
    }
    hash
}

/// Differing tables can use the owner's texture hash only after the same model/sound
/// compatibility check that authorized the name mapping in `sync_vars`.
pub fn script_texture_subscription_table(
    bus: &str,
    program: &omsi_script::Program,
    surfaces: &[(i32, i32)],
    schema: Option<&omsi_net::var_schema::Schema>,
    mapped: Option<&VarTable>,
) -> Option<u32> {
    let local = var_table(program).hash;
    let owner = match schema {
        Some(schema) if schema.table_hash() != local => {
            mapped
                .filter(|table| table.hash == schema.table_hash())?
                .hash
        }
        _ => local,
    };
    Some(script_texture_table_for_vars(bus, owner, surfaces))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variable_schema;
    #[test]
    fn script_texture_tables_require_the_same_bus_surfaces_and_scripts() {
        let mut program = omsi_script::Program::default();
        program.declare_script_var("display_mode");
        let surfaces = [(128, 32), (64, 16)];
        let hash = script_texture_table("Vehicles/Bus/main.bus", &program, &surfaces);
        assert_eq!(
            hash,
            script_texture_table("vehicles\\bus\\MAIN.bus", &program, &surfaces)
        );
        assert_ne!(
            hash,
            script_texture_table("Vehicles/Other/main.bus", &program, &surfaces)
        );
        assert_ne!(
            hash,
            script_texture_table("Vehicles/Bus/main.bus", &program, &[(128, 64), (64, 16)])
        );
        assert_ne!(
            hash,
            script_texture_table("Vehicles/Bus/main.bus", &program, &surfaces[..1])
        );
        program.declare_script_var("other_display_mode");
        assert_ne!(
            hash,
            script_texture_table("Vehicles/Bus/main.bus", &program, &surfaces)
        );
    }

    fn texture_test_schema(program: &omsi_script::Program) -> omsi_net::var_schema::Schema {
        let table = var_table(program);
        omsi_net::var_schema::Schema {
            floats: table
                .floats
                .iter()
                .map(|i| program.var_names[*i as usize].to_ascii_lowercase())
                .collect(),
            strings: table
                .strings
                .iter()
                .map(|i| program.str_var_names[*i as usize].to_ascii_lowercase())
                .collect(),
            model: vec!["f:display_mode".into()],
            sound: vec!["f:display_mode".into()],
        }
    }

    #[test]
    fn compatible_script_texture_layout_uses_the_owners_variable_hash() {
        let mut local = omsi_script::Program::default();
        local.declare_script_var("display_mode");
        let mut owner = local.clone();
        owner.declare_script_var("internal_extra");
        owner.declare_str_var("unused_string");
        let local_schema = texture_test_schema(&local);
        let owner_schema = texture_test_schema(&owner);
        let mapping = variable_schema::mapping(&local, &local_schema, &owner_schema)
            .expect("compatible configs");
        let bus = "Vehicles/Bus/main.bus";
        let surfaces = [(128, 32), (64, 16)];
        let expected = script_texture_table(bus, &owner, &surfaces);
        assert_ne!(expected, script_texture_table(bus, &local, &surfaces));
        assert_eq!(
            script_texture_subscription_table(
                bus,
                &local,
                &surfaces,
                Some(&owner_schema),
                Some(&mapping)
            ),
            Some(expected)
        );
        // An unchecked or missing mapping never opts a differing table into pixel transfer.
        assert_eq!(
            script_texture_subscription_table(bus, &local, &surfaces, Some(&owner_schema), None),
            None
        );
        let stale = var_table(&local);
        assert_eq!(
            script_texture_subscription_table(
                bus,
                &local,
                &surfaces,
                Some(&owner_schema),
                Some(&stale)
            ),
            None
        );
        // Declared slot counts and sizes still form part of the subscription table.
        assert_ne!(
            script_texture_subscription_table(
                bus,
                &local,
                &surfaces[..1],
                Some(&owner_schema),
                Some(&mapping)
            ),
            Some(expected)
        );
        assert_ne!(
            script_texture_subscription_table(
                bus,
                &local,
                &[(256, 32), (64, 16)],
                Some(&owner_schema),
                Some(&mapping)
            ),
            Some(expected)
        );
    }

    #[test]
    fn differing_script_texture_tables_reject_incompatible_model_or_sound_refs() {
        let mut local = omsi_script::Program::default();
        local.declare_script_var("display_mode");
        let mut owner = local.clone();
        owner.declare_script_var("extra");
        let local_schema = texture_test_schema(&local);
        for sound in [false, true] {
            let mut schema = texture_test_schema(&owner);
            if sound {
                schema.sound.push("f:extra".into());
            } else {
                schema.model.push("f:extra".into());
            }
            let mapping = variable_schema::mapping(&local, &local_schema, &schema);
            assert!(mapping.is_none());
            assert_eq!(
                script_texture_subscription_table(
                    "bus.bus",
                    &local,
                    &[(128, 32)],
                    Some(&schema),
                    mapping.as_ref()
                ),
                None
            );
        }
    }
}
