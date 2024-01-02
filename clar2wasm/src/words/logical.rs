use clarity::vm::types::TypeSignature;
use clarity::vm::ClarityName;

use super::SimpleWord;
use crate::wasm_generator::{GeneratorError, WasmGenerator};

#[derive(Debug)]
pub struct Not;

impl SimpleWord for Not {
    fn name(&self) -> ClarityName {
        "not".into()
    }

    fn traverse(
        &self,
        generator: &mut WasmGenerator,
        builder: &mut walrus::InstrSeqBuilder,
        _arg_types: &[TypeSignature],
        _return_type: &TypeSignature,
    ) -> Result<(), GeneratorError> {
        builder.call(generator.func_by_name("stdlib.not"));
        Ok(())
    }
}
