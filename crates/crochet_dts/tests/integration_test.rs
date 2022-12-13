use error_stack::Report;
use std::fs;

use crochet_ast::values::Program;
use crochet_dts::parse_dts::*;
use crochet_infer::expand_type;
use crochet_parser::parse;

use core::{any::TypeId, panic::Location};
use error_stack::{AttachmentKind, FrameKind};

pub fn messages<E>(report: &Report<E>) -> Vec<String> {
    report
        .frames()
        .map(|frame| match frame.kind() {
            FrameKind::Context(context) => context.to_string(),
            FrameKind::Attachment(AttachmentKind::Printable(attachment)) => attachment.to_string(),
            FrameKind::Attachment(AttachmentKind::Opaque(_)) => {
                #[cfg(all(rust_1_65, feature = "std"))]
                if frame.type_id() == TypeId::of::<Backtrace>() {
                    return String::from("Backtrace");
                }
                #[cfg(feature = "spantrace")]
                if frame.type_id() == TypeId::of::<SpanTrace>() {
                    return String::from("SpanTrace");
                }
                if frame.type_id() == TypeId::of::<Location>() {
                    String::from("Location")
                } else {
                    String::from("opaque")
                }
            }
            FrameKind::Attachment(_) => panic!("attachment was not covered"),
        })
        .collect()
}

static LIB_ES5_D_TS: &str = "../../node_modules/typescript/lib/lib.es5.d.ts";

fn infer_prog(src: &str) -> (Program, crochet_infer::Context) {
    let lib = fs::read_to_string(LIB_ES5_D_TS).unwrap();
    let mut ctx = parse_dts(&lib).unwrap();

    let result = parse(src);
    let mut prog = match result {
        Ok(prog) => prog,
        Err(err) => {
            println!("err = {:?}", err);
            panic!("Error parsing expression");
        }
    };
    let ctx = crochet_infer::infer_prog(&mut prog, &mut ctx).unwrap();

    (prog, ctx)
}

fn infer_prog_with_type_error(lib: &str, src: &str) -> Vec<String> {
    let mut ctx = parse_dts(lib).unwrap();

    let result = parse(src);
    let mut prog = match result {
        Ok(prog) => prog,
        Err(err) => {
            println!("err = {:?}", err);
            panic!("Error parsing expression");
        }
    };

    match crochet_infer::infer_prog(&mut prog, &mut ctx) {
        Ok(_) => panic!("was expect infer_prog() to return an error"),
        Err(report) => messages(&report),
    }
}

#[test]
fn infer_adding_variables() {
    let src = r#"
    let msg = "Hello, world!";
    let len = msg.length.toString(); // radix is optional
    "#;
    let (_, ctx) = infer_prog(src);
    let result = format!("{}", ctx.lookup_value("len").unwrap());
    assert_eq!(result, "string");
}

#[test]
fn infer_method_on_readonly_array() {
    let src = r#"
    declare let arr: string[];
    let map = arr.map;
    "#;
    let (_, ctx) = infer_prog(src);
    let result = format!("{}", ctx.lookup_value("map").unwrap());
    assert_eq!(
        result,
        "<t0, t1>(callbackfn: (value: string, index: number, array: mut string[]) => t0, thisArg?: t1) => mut t0[]"
    );
}

#[test]
#[should_panic = "Object type doesn't contain key splice."]
fn infer_mutable_method_on_readonly_array_errors() {
    let src = r#"
    declare let arr: string[];
    let splice = arr.splice;
    "#;

    infer_prog(src);
}

#[test]
fn infer_method_on_readonly_arrays_of_different_things() {
    let src = r#"
    declare let str_arr: string[];
    declare let num_arr: number[];
    let map1 = str_arr.map;
    let map2 = num_arr.map;
    "#;
    let (_, ctx) = infer_prog(src);

    let result = format!("{}", ctx.lookup_value("map1").unwrap());
    assert_eq!(
        result,
        "<t0, t1>(callbackfn: (value: string, index: number, array: mut string[]) => t0, thisArg?: t1) => mut t0[]"
    );
    let result = format!("{}", ctx.lookup_value("map2").unwrap());
    assert_eq!(
        result,
        "<t0, t1>(callbackfn: (value: number, index: number, array: mut number[]) => t0, thisArg?: t1) => mut t0[]"
    );
}

#[test]
fn infer_method_on_mutable_array() {
    let src = r#"
    declare let mut_arr: mut string[];
    let sort = mut_arr.sort;
    let splice = mut_arr.splice;
    let sorted_arr = mut_arr.sort();
    "#;
    let (_, ctx) = infer_prog(src);

    let result = format!("{}", ctx.lookup_value("sort").unwrap());
    assert_eq!(
        result,
        "(compareFn?: (a: string, b: string) => number) => mut string[]"
    );
    let result = format!("{}", ctx.lookup_value("splice").unwrap());
    assert_eq!(
        result,
        "(start: number, deleteCount?: number) => mut string[]"
    );
    let result = format!("{}", ctx.lookup_value("sorted_arr").unwrap());
    assert_eq!(result, "mut string[]");
}

#[test]
fn infer_array_method_on_tuple() {
    let src = r#"
    let tuple = [5, "hello", true];
    let map = tuple.map;
    "#;
    let (_, ctx) = infer_prog(src);
    let result = format!("{}", ctx.lookup_value("map").unwrap());
    assert_eq!(
        result,
        // TODO: add parens around a union when it's the child of an arry
        "<t0, t1>(callbackfn: (value: \"hello\" | 5 | true, index: number, array: mut \"hello\" | 5 | true[]) => t0, thisArg?: t1) => mut t0[]"
    );
}

#[test]
fn infer_static_properties() {
    let src = r#"
    let max = Number.MAX_VALUE;
    let parse = Date.parse;
    "#;
    let (_, ctx) = infer_prog(src);

    let result = format!("{}", ctx.lookup_value("max").unwrap());
    assert_eq!(result, "number");
    let result = format!("{}", ctx.lookup_value("parse").unwrap());
    assert_eq!(result, "(s: string) => number");
}

#[test]
fn infer_callable_results_on_interface() {
    let lib = r#"
    interface Foo {
        (x: number): number;
        (x: string): string;
        bar: boolean;
    }
    "#;
    let mut ctx = parse_dts(lib).unwrap();

    let src = r#"
    declare let foo: Foo;
    let num = foo(5);
    let str = foo("hello");
    let bool = foo.bar;
    "#;
    let result = parse(src);
    let mut prog = match result {
        Ok(prog) => prog,
        Err(err) => {
            println!("err = {:?}", err);
            panic!("Error parsing expression");
        }
    };
    let ctx = crochet_infer::infer_prog(&mut prog, &mut ctx).unwrap();

    let result = format!("{}", ctx.lookup_value("num").unwrap());
    assert_eq!(result, "number");
    let result = format!("{}", ctx.lookup_value("str").unwrap());
    assert_eq!(result, "string");
    let result = format!("{}", ctx.lookup_value("bool").unwrap());
    assert_eq!(result, "boolean");
}

// TODO: Write a test for parametric callables

#[test]
fn infer_index_value_on_interface() {
    let lib = r#"
    interface Foo {
        [x: number]: number;
        bar: boolean;
    }
    "#;
    let mut ctx = parse_dts(lib).unwrap();

    let src = r#"
    declare let foo: Foo;
    let num = foo[5];
    let bool = foo.bar;
    "#;
    let result = parse(src);
    let mut prog = match result {
        Ok(prog) => prog,
        Err(err) => {
            println!("err = {:?}", err);
            panic!("Error parsing expression");
        }
    };
    let ctx = crochet_infer::infer_prog(&mut prog, &mut ctx).unwrap();

    let result = format!("{}", ctx.lookup_value("num").unwrap());
    assert_eq!(result, "number | undefined");
    let result = format!("{}", ctx.lookup_value("bool").unwrap());
    assert_eq!(result, "boolean");
}

#[test]
fn infer_generic_index_value_on_interface() {
    let lib = r#"
    interface Foo {
        [x: number]: <T>(arg: T) => T;
        bar: boolean;
    }
    "#;
    let mut ctx = parse_dts(lib).unwrap();

    let src = r#"
    declare let foo: Foo;
    let id = foo[5];
    "#;
    let result = parse(src);
    let mut prog = match result {
        Ok(prog) => prog,
        Err(err) => {
            println!("err = {:?}", err);
            panic!("Error parsing expression");
        }
    };
    let ctx = crochet_infer::infer_prog(&mut prog, &mut ctx).unwrap();

    let result = format!("{}", ctx.lookup_value("id").unwrap());
    // NOTE: The type variables aren't normalized.  See comment inside
    // norm_type() in crochet_infer/src/util.rs.
    assert_eq!(result, "<t1>(arg: t1) => t1 | undefined");
}

#[test]
// #[should_panic = "\\\"hello\\\" is an invalid key for object types"]
fn infer_index_with_incorrect_key_type_on_interface() {
    let lib = r#"
    interface Foo {
        [x: number]: number;
        bar: boolean;
    }
    "#;

    let src = r#"
    declare let foo: Foo;
    let num = foo["hello"];
    let bool = foo.bar;
    "#;

    let error_messages = infer_prog_with_type_error(lib, src);

    assert_eq!(
        error_messages,
        vec![
            "\"hello\" is an invalid key for object types",
            "Location",
            "TypeError::InvalidKey: \"hello\" is not a valid key"
        ]
    );
}

#[test]
fn instantiating_generic_interfaces() {
    let lib = r#"
    interface Foo<T> {
        bar(x: T): any;
        baz(x: T): any;
    }
    "#;
    let mut ctx = parse_dts(lib).unwrap();

    let src = r#"
    declare let foo: Foo<number>;
    let bar = foo.bar;
    "#;
    let result = parse(src);
    let mut prog = match result {
        Ok(prog) => prog,
        Err(err) => {
            println!("err = {:?}", err);
            panic!("Error parsing expression");
        }
    };
    let ctx = crochet_infer::infer_prog(&mut prog, &mut ctx).unwrap();

    let result = format!("{}", ctx.lookup_value("bar").unwrap());
    assert_eq!(result, "<t0>(x: number) => t0");
}

#[test]
fn interface_with_generic_method() {
    let lib = r#"
    interface Foo<T> {
        bar<U>(x: U): U;
        baz: T;
    }
    "#;
    let mut ctx = parse_dts(lib).unwrap();

    let src = r#"
    declare let foo: Foo<number>;
    let bar = foo.bar;
    "#;
    let result = parse(src);
    let mut prog = match result {
        Ok(prog) => prog,
        Err(err) => {
            println!("err = {:?}", err);
            panic!("Error parsing expression");
        }
    };
    let ctx = crochet_infer::infer_prog(&mut prog, &mut ctx).unwrap();

    let result = format!("{}", ctx.lookup_value("bar").unwrap());
    assert_eq!(result, "<t0>(x: t0) => t0");
}

#[test]
fn merging_generic_interfaces() {
    let lib = r#"
    interface Foo<T> {
        bar(x: T): number;
    }

    interface Foo<T> {
        baz(x: T): string;
    }
    "#;
    let mut ctx = parse_dts(lib).unwrap();

    let src = r#"
    declare let foo: Foo<number>;
    "#;
    let result = parse(src);
    let mut prog = match result {
        Ok(prog) => prog,
        Err(err) => {
            println!("err = {:?}", err);
            panic!("Error parsing expression");
        }
    };
    let ctx = crochet_infer::infer_prog(&mut prog, &mut ctx).unwrap();

    let result = format!("{}", ctx.lookup_type("Foo", false).unwrap());
    assert_eq!(
        result,
        "<t0>{bar: (x: t0) => number, baz: (x: t0) => string}"
    );
}

#[test]
fn infer_partial() {
    let src = r#"
    type Obj = {a: number, b?: string, mut c: boolean, mut d?: number};
    type PartialObj = Partial<Obj>;
    "#;
    let (_, ctx) = infer_prog(src);
    let t = ctx.lookup_type("PartialObj", false).unwrap();
    let t = expand_type(&t, &ctx).unwrap();

    let result = format!("{}", t);
    assert_eq!(
        result,
        "{a?: number, b?: string, mut c?: boolean, mut d?: number}"
    );
}

#[test]
fn infer_required() {
    let src = r#"
    type Obj = {a: number, b?: string, mut c: boolean, mut d?: number};
    type RequiredObj = Required<Obj>;
    "#;
    let (_, ctx) = infer_prog(src);
    let t = ctx.lookup_type("RequiredObj", false).unwrap();
    let t = expand_type(&t, &ctx).unwrap();

    let result = format!("{}", t);
    assert_eq!(
        result,
        "{a: number, b: string, mut c: boolean, mut d: number}"
    );
}

#[test]
fn infer_readonly() {
    let src = r#"
    type Obj = {a: number, b?: string, mut c: boolean, mut d?: number};
    type ReadonlyObj = Readonly<Obj>;
    "#;
    let (_, ctx) = infer_prog(src);
    let t = ctx.lookup_type("ReadonlyObj", false).unwrap();
    let t = expand_type(&t, &ctx).unwrap();

    let result = format!("{}", t);
    assert_eq!(result, "{a: number, b?: string, c: boolean, d?: number}");
}

#[test]
fn infer_readonly_with_indexer_only() {
    let src = r#"
    type Obj = {[key: string]: boolean};
    type ReadonlyObj = Readonly<Obj>;
    "#;
    let (_, ctx) = infer_prog(src);
    let t = ctx.lookup_type("ReadonlyObj", false).unwrap();
    let t = expand_type(&t, &ctx).unwrap();

    let result = format!("{}", t);
    assert_eq!(result, "{[key: string]: boolean}");
}

#[test]
fn infer_readonly_with_indexer_and_other_properties() {
    let src = r#"
    type Obj = {a: number, b?: string, mut c: boolean, mut d?: number, [key: number]: boolean};
    type ReadonlyObj = Readonly<Obj>;
    "#;
    let (_, ctx) = infer_prog(src);
    let t = ctx.lookup_type("ReadonlyObj", false).unwrap();
    let t = expand_type(&t, &ctx).unwrap();

    let result = format!("{}", t);
    assert_eq!(
        result,
        "{[key: number]: boolean, a: number, b?: string, c: boolean, d?: number}"
    );
}

#[test]
fn infer_pick() {
    let src = r#"
    type Obj = {a: number, b?: string, mut c: boolean, mut d?: number};
    type PickObj = Pick<Obj, "a" | "b">;
    "#;
    let (_, ctx) = infer_prog(src);
    let t = ctx.lookup_type("PickObj", false).unwrap();
    let t = expand_type(&t, &ctx).unwrap();

    let result = format!("{}", t);
    assert_eq!(result, "{a: number, b?: string}");
}

#[test]
fn infer_prog_using_partial() {
    let src = r#"
    type Obj = {a: number, b?: string, mut c: boolean, mut d?: number};
    type PartialObj = Partial<Obj>;

    let partial_obj: PartialObj = {b: "hello"};
    "#;

    infer_prog(src);
}

#[test]
fn tuple_mapping() {
    let src = r#"
    let tuple = [1, 2, 3];
    let squares = tuple.map((x) => x * x);
    let sqr_fn = (x) => x * x;
    let squares2 = [1, 2, 3].map(sqr_fn);
    let squares3 = [1, 2, 3].map((x) => x * x);
    "#;

    let (_, ctx) = infer_prog(src);

    let t = ctx.lookup_value("squares").unwrap();
    let result = format!("{}", t);
    assert_eq!(result, "mut number[]");
}

#[test]
fn infer_exclude() {
    let src = r#"
    type T1 = Exclude<"a" | "b" | "c", "a" | "b">;
    "#;
    let (_, ctx) = infer_prog(src);
    let t = ctx.lookup_type("T1", false).unwrap();

    let result = format!("{}", t);
    assert_eq!(result, "Exclude<\"a\" | \"b\" | \"c\", \"a\" | \"b\">");

    let t = expand_type(&t, &ctx).unwrap();
    let result = format!("{}", t);

    assert_eq!(result, "\"c\"");
}

#[test]
fn infer_out_of_order_exclude() {
    let lib = r#"
    type Exclude<U, T> = T extends U ? never : T;
    "#;
    let mut ctx = parse_dts(lib).unwrap();

    let src = r#"
    type T1 = Exclude<"a" | "b", "a" | "b" | "c">;
    "#;
    let result = parse(src);
    let mut prog = match result {
        Ok(prog) => prog,
        Err(err) => {
            println!("err = {:?}", err);
            panic!("Error parsing expression");
        }
    };
    let ctx = crochet_infer::infer_prog(&mut prog, &mut ctx).unwrap();

    let t = ctx.lookup_type("T1", false).unwrap();
    let t = expand_type(&t, &ctx).unwrap();
    let result = format!("{}", t);
    assert_eq!(result, "\"c\"");
}

#[test]
fn infer_omit() {
    let src = r#"
    type Obj = {a: number, b?: string, mut c: boolean, mut d?: number};
    type T1 = Omit<Obj, "b" | "c">;
    "#;
    let (_, ctx) = infer_prog(src);
    let t = ctx.lookup_type("T1", false).unwrap();

    let result = format!("{}", t);
    assert_eq!(result, "Omit<Obj, \"b\" | \"c\">");

    let t = expand_type(&t, &ctx).unwrap();
    let result = format!("{}", t);

    assert_eq!(result, "{a: number, mut d?: number}");
}

#[test]
fn new_expressions() {
    let src = r#"
    let mut array = new Array(1, 2, 3);
    "#;

    let (_, ctx) = infer_prog(src);

    let t = ctx.lookup_value("array").unwrap();
    let result = format!("{}", t);
    assert_eq!(result, "mut 1 | 2 | 3[]");
}

#[test]
fn new_expressions_instantiation_check() {
    let src = r#"
    let numbers = new Array(1, 2, 3);
    let letters = new Array("a", "b", "c");
    "#;

    let (_, ctx) = infer_prog(src);

    let t = ctx.lookup_value("numbers").unwrap();
    let result = format!("{}", t);
    assert_eq!(result, "mut 1 | 2 | 3[]");

    let t = ctx.lookup_value("letters").unwrap();
    let result = format!("{}", t);
    assert_eq!(result, r#"mut "a" | "b" | "c"[]"#);
}

#[test]
fn rest_fn() {
    let src = r#"
    declare let foo: <T>(...items: mut T[]) => string;
    let result = foo(1, 2, 3);
    "#;

    let (_, ctx) = infer_prog(src);

    let t = ctx.lookup_value("result").unwrap();
    let result = format!("{}", t);
    assert_eq!(result, "string");
}
