use clar2wasm::tools::crosscheck_compare_only;
use clarity::vm::types::{ListTypeData, SequenceSubtype, StringSubtype, TypeSignature};
use proptest::strategy::{Just, Strategy};
use proptest::{prop_oneof, proptest};

use crate::{list, PropValue, Value};

fn strategies_for_list() -> impl Strategy<Value = TypeSignature> {
    prop_oneof![
        Just(TypeSignature::IntType),
        Just(TypeSignature::UIntType),
        (0u32..128).prop_map(|s| TypeSignature::SequenceType(SequenceSubtype::BufferType(
            s.try_into().unwrap()
        ))),
        (0u32..128).prop_map(|s| TypeSignature::SequenceType(SequenceSubtype::StringType(
            StringSubtype::ASCII(s.try_into().unwrap())
        )))
    ]
}

fn generate_list(ty: TypeSignature) -> impl Strategy<Value = Value> {
    (8u32..32)
        .prop_map(move |s| ListTypeData::new_list(ty.clone(), s).unwrap())
        .prop_flat_map(list)
}

proptest! {
    #![proptest_config(super::runtime_config())]

    #[test]
    fn crossprop_map_list(
        vals in strategies_for_list()
        .prop_map(generate_list)
        .prop_flat_map(|l| proptest::collection::vec(l, 1..=10))) {

        for chunk in vals.chunks(2) {
            if let [arg1, arg2] = chunk {
                let cmd = format!(
                    "(map <= {} {})",
                    arg1.to_string().replace('(', "(list "),
                    arg2.to_string().replace('(', "(list ")
                );

                crosscheck_compare_only(&cmd);
            }
        }
    }
}

proptest! {
    #![proptest_config(super::runtime_config())]

    #[test]
    fn crossprop_map_seq(vals in PropValue::any_sequence(20usize)) {
        crosscheck_compare_only(
            &format!("(map + (list (len {vals})))")
        )
    }
}
