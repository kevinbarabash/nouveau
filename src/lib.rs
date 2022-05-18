pub mod ast;
pub mod codegen;
pub mod infer;
pub mod parser; 
pub mod types;

use chumsky::*;
use wasm_bindgen::prelude::*;
use std::collections::HashMap;

use crate::infer::{Env, infer_prog};

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

fn greet(msg: &str) {
    alert(msg);
}

#[wasm_bindgen]
pub fn foo() {
    greet("Hello, world!");
}

#[wasm_bindgen]
pub struct CompileResult {
    js: String,
    dts: String,
}

#[wasm_bindgen]
impl CompileResult {
    // #[wasm_bindgen(constructor)]
    // pub fn new(field: i32) -> Baz {
    //     Baz { field }
    // }

    #[wasm_bindgen(getter)]
    pub fn js(&self) -> String {
        self.js.to_owned()
    }

    #[wasm_bindgen(getter)]
    pub fn dts(&self) -> String {
        self.dts.to_owned()
    }
}

#[wasm_bindgen]
pub fn compile(input: &str) -> CompileResult {
    let program = parser::parser().parse(input).unwrap();

    let js_program = codegen::js::build_js(&program);
    let js = codegen::js::print_js(&js_program);

    let env: Env = HashMap::new();
    let env = infer_prog(env, &program);
    let program = codegen::d_ts::build_d_ts(&program, &env);
    let dts = codegen::d_ts::print_d_ts(&program);

    CompileResult { js, dts }
}
