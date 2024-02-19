use clar2wasm::tools::{crosscheck, crosscheck_compare_only};
use clarity::vm::types::{ListTypeData, SequenceSubtype, StringSubtype, TypeSignature};
use proptest::strategy::{Just, Strategy};
use proptest::{prop_oneof, proptest};

use crate::{list, uint, PropValue, TypePrinter, Value};

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
    fn crossprop_map_lte(
        vals in strategies_for_list()
        .prop_map(generate_list)
        .prop_flat_map(|l| proptest::collection::vec(l, 1..=10))) {

        for chunk in vals.chunks(2) {
            if let [arg1, arg2] = chunk {
                let cmd = format!(
                    "(map <= {} {})",
                    PropValue(arg1.clone()),
                    PropValue(arg2.clone())
                );

                crosscheck_compare_only(&cmd);
            }
        }
    }
}

proptest! {
    #![proptest_config(super::runtime_config())]

    #[test]
    fn crossprop_map_len(vals in PropValue::any_sequence(20usize)) {
        crosscheck_compare_only(
            &format!("(map + (list (len {vals})))")
        )
    }
}

proptest! {
    #![proptest_config(super::runtime_config())]

    #[test]
    fn crossprop_map_is_eq(vals in PropValue::any_sequence(5usize)) {
        let snippet = &format!("(and (fold or (map not (list (is-eq {vals} {vals}) (is-eq {vals} {vals}))) true))");

        crosscheck(snippet, Ok(Some(Value::Bool(true))));
    }
}

proptest! {
    #![proptest_config(super::runtime_config())]

    #[test]
    fn crossprop_datamap_insert(key in uint(), vals in PropValue::any_sequence(20usize)) {
        let vals_type_str = vals.type_string();
        crosscheck_compare_only(
            &format!(
                r#"
                  (define-map mp {{x: uint}} {{y: {vals_type_str} }})
                  (map-insert mp {{x: {key}}} {{y: {vals}}})
                "#
            )
        )
    }
}

proptest! {
    #![proptest_config(super::runtime_config())]

    #[test]
    fn crossprop_datamap_set(key in uint(), vals in PropValue::any_sequence(20usize)) {
        let vals_type_str = vals.type_string();
        crosscheck_compare_only(
            &format!(
                r#"
                  (define-map mp {{x: uint}} {{y: {vals_type_str} }})
                  (map-set mp {{x: {key}}} {{y: {vals}}})
                "#
            )
        )
    }
}

proptest! {
    #![proptest_config(super::runtime_config())]

    #[test]
    fn crossprop_datamap_delete(key in uint(), vals in PropValue::any_sequence(20usize)) {
        let vals_type_str = vals.type_string();
        crosscheck_compare_only(
            &format!(
                r#"
                  (define-map mp {{x: uint}} {{y: {vals_type_str} }})
                  (map-set mp {{x: {key}}} {{y: {vals}}})
                  (map-delete mp {{ x: {key} }})
                "#
            )
        )
    }
}
