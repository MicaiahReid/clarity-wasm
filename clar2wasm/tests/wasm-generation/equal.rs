use clar2wasm::tools::crosscheck;
use clarity::vm::types::OptionalData;
use clarity::vm::Value;
use proptest::proptest;

use crate::PropValue;

proptest! {
    #![proptest_config(super::runtime_config())]

    #[test]
    fn is_eq_one_argument_always_true(val in PropValue::any()) {
        crosscheck(
            &format!(r#"(is-eq {val})"#),
            Ok(Some(clarity::vm::Value::Bool(true)))
        );
    }
}

proptest! {
    #![proptest_config(super::runtime_config())]

    #[test]
    fn is_eq_value_with_itself_always_true(val in PropValue::any()) {
        crosscheck(
            &format!(r#"(is-eq {val} {val})"#),
            Ok(Some(clarity::vm::Value::Bool(true)))
        );
    }
}

proptest! {
    #![proptest_config(super::runtime_config())]

    #[test]
    fn is_eq_value_with_itself_always_true_3(val in PropValue::any()) {
        crosscheck(
            &format!(r#"(is-eq {val} {val} {val})"#),
            Ok(Some(clarity::vm::Value::Bool(true)))
        );
    }
}

proptest! {
    #![proptest_config(super::runtime_config())]

    #[test]
    fn crossprop_index_of(seq in PropValue::any_sequence(20usize)) {
        crosscheck(&format!(
            "(index-of? {seq} (try! (element-at? {seq} (- (len {seq}) (len {seq})))))"),
            Ok(Some(Value::Optional(OptionalData {
                data: Some(Box::new(Value::UInt(0)))
            })))
        )
    }
}
