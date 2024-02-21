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
    (1..=32u32)
        .prop_map(move |s| ListTypeData::new_list(ty.clone(), s).unwrap())
        .prop_flat_map(list)
}

proptest! {
    #![proptest_config(super::runtime_config())]

    #[test]
    fn crossprop_map_lte(
        (list1, list2) in strategies_for_list()
        .prop_flat_map(|ty| (generate_list(ty.clone()), generate_list(ty.clone())))
        .prop_map(|(l1, l2)| (PropValue::from(l1), PropValue(l2)))) {

            crosscheck_compare_only(&format!("(map <= {list1} {list2})"));
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
