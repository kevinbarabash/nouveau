use std::fs;

use escalier_hm::checker::Checker;
use escalier_hm::context::Context;
use escalier_hm::type_error::TypeError;
use escalier_interop::parse::*;
use escalier_parser::parse;

pub fn messages(report: &[TypeError]) -> Vec<String> {
    report.iter().map(|error| error.to_string()).collect()
}

static LIB_ES5_D_TS: &str = "../../node_modules/typescript/lib/lib.es5.d.ts";

fn infer_prog(src: &str) -> (Checker, Context) {
    let lib = fs::read_to_string(LIB_ES5_D_TS).unwrap();
    let (mut checker, mut ctx) = parse_dts(&lib).unwrap();

    let result = parse(src);
    let mut prog = match result {
        Ok(prog) => prog,
        Err(err) => {
            println!("err = {:?}", err);
            panic!("Error parsing expression");
        }
    };
    match checker.infer_program(&mut prog, &mut ctx) {
        Ok(_) => {
            if !checker.current_report.diagnostics.is_empty() {
                panic!("was expecting infer_prog() to return no errors");
            }
            (checker, ctx)
        }
        Err(error) => {
            let message = error.to_string();
            panic!("{message}");
        }
    }
}

// fn infer_prog_with_type_error(lib: &str, src: &str) -> Vec<String> {
//     let mut checker = parse_dts(lib).unwrap();
//     let mut ctx = Context::default();

//     let result = parse(src);
//     let mut prog = match result {
//         Ok(prog) => prog,
//         Err(err) => {
//             println!("err = {:?}", err);
//             panic!("Error parsing expression");
//         }
//     };
//     match checker.infer_program(&mut prog) {
//         Ok(_) => panic!("was expect infer_prog() to return an error"),
//         Err(report) => messages(&report),
//     }
// }

#[test]
fn infer_adding_variables() {
    let src = r#"
    let msg = "Hello, world!"
    let len = msg.length.toString() // radix is optional
    "#;
    let (checker, ctx) = infer_prog(src);
    let binding = ctx.get_binding("len").unwrap();
    let result = checker.print_type(&binding.index);
    assert_eq!(result, "string");
}

#[test]
fn infer_method_on_readonly_array() {
    let src = r#"
    declare let arr: string[]
    let map = arr.map
    "#;
    let (checker, ctx) = infer_prog(src);
    let binding = ctx.get_binding("map").unwrap();
    let result = checker.print_type(&binding.index);
    assert_eq!(
        result,
        "<U, A>(self: Self, callbackfn: (value: string, index: number, array: string[]) -> U, thisArg?: A) -> U[]"
    );
}

#[test]
#[should_panic = "TypeError: Cannot call mutating method splice on a non-mutable object"]
fn infer_mutable_method_on_readonly_array_errors() {
    let src = r#"
    declare let arr: string[]
    let splice = arr.splice
    "#;

    infer_prog(src);
}

#[test]
fn infer_method_on_readonly_arrays_of_different_things() {
    let src = r#"
    declare let str_arr: string[]
    declare let num_arr: number[]
    let map1 = str_arr.map
    let map2 = num_arr.map
    "#;
    let (checker, ctx) = infer_prog(src);

    let binding = ctx.get_binding("map1").unwrap();
    let result = checker.print_type(&binding.index);
    assert_eq!(
        result,
        "<U, A>(self: Self, callbackfn: (value: string, index: number, array: string[]) -> U, thisArg?: A) -> U[]"
    );

    let binding = ctx.get_binding("map2").unwrap();
    let result = checker.print_type(&binding.index);
    assert_eq!(
        result,
        "<U, A>(self: Self, callbackfn: (value: number, index: number, array: number[]) -> U, thisArg?: A) -> U[]"
    );
}

#[test]
fn infer_method_on_mutable_array() {
    let src = r#"
    declare let mut mut_arr: string[]
    let sort = mut_arr.sort
    let splice = mut_arr.splice
    let sorted_arr = mut_arr.sort()
    "#;
    let (checker, ctx) = infer_prog(src);

    let binding = ctx.get_binding("sort").unwrap();
    let result = checker.print_type(&binding.index);
    assert_eq!(
        result,
        "(mut self: Self, compareFn?: (a: string, b: string) -> number) -> Array<string>"
    );
    let binding = ctx.get_binding("splice").unwrap();
    let result = checker.print_type(&binding.index);
    assert_eq!(
        result,
        "(mut self: Self, start: number, deleteCount?: number) -> string[]"
    );
    // TODO: Don't expand the type unless required
    // let binding = ctx.get_binding("sorted_arr").unwrap();
    // let result = checker.print_type(&binding.index);
    // assert_eq!(result, "mut string[]");
}

#[test]
fn infer_array_method_on_tuple() {
    let src = r#"
    let tuple = [5, "hello", true]
    let map = tuple.map
    "#;
    let (checker, ctx) = infer_prog(src);
    let binding = ctx.get_binding("map").unwrap();
    let result = checker.print_type(&binding.index);
    assert_eq!(
        result,
        // TODO: add parens around a union when it's the child of an arry
        "<U, A>(self: Self, callbackfn: (value: 5 | \"hello\" | true, index: number, array: 5 | \"hello\" | true[]) -> U, thisArg?: A) -> U[]"
    );
}

#[test]
fn infer_static_properties() {
    let src = r#"
    let max = Number.MAX_VALUE
    let parse = Date.parse
    "#;
    let (checker, ctx) = infer_prog(src);

    let binding = ctx.values.get("max").unwrap();
    let result = checker.print_type(&binding.index);
    assert_eq!(result, "number");
    let binding = ctx.values.get("parse").unwrap();
    let result = checker.print_type(&binding.index);
    assert_eq!(result, "(s: string) -> number");
}

// #[test]
// fn infer_callable_results_on_interface() {
//     let lib = r#"
//     interface Foo {
//         (x: number): number;
//         (x: string): string;
//         bar: boolean;
//     }
//     "#;
//     let mut checker = parse_dts(lib).unwrap();

//     let src = r#"
//     declare let foo: Foo;
//     let num = foo(5);
//     let str = foo("hello");
//     let bool = foo.bar;
//     "#;
//     let result = parse(src);
//     let mut prog = match result {
//         Ok(prog) => prog,
//         Err(err) => {
//             println!("err = {:?}", err);
//             panic!("Error parsing expression");
//         }
//     };
//     escalier_old_infer::infer_prog(&mut prog, &mut checker).unwrap();

//     let result = format!("{}", checker.lookup_value("num").unwrap());
//     assert_eq!(result, "number");
//     let result = format!("{}", checker.lookup_value("str").unwrap());
//     assert_eq!(result, "string");
//     let result = format!("{}", checker.lookup_value("bool").unwrap());
//     assert_eq!(result, "boolean");
// }

// // TODO: Write a test for parametric callables

// #[test]
// fn infer_index_value_on_interface() {
//     let lib = r#"
//     interface Foo {
//         [x: number]: number;
//         bar: boolean;
//     }
//     "#;
//     let mut checker = parse_dts(lib).unwrap();

//     let src = r#"
//     declare let foo: Foo;
//     let num = foo[5];
//     let bool = foo.bar;
//     "#;
//     let result = parse(src);
//     let mut prog = match result {
//         Ok(prog) => prog,
//         Err(err) => {
//             println!("err = {:?}", err);
//             panic!("Error parsing expression");
//         }
//     };

//     escalier_old_infer::infer_prog(&mut prog, &mut checker).unwrap();

//     let result = format!("{}", checker.lookup_value("num").unwrap());
//     assert_eq!(result, "number | undefined");
//     let result = format!("{}", checker.lookup_value("bool").unwrap());
//     assert_eq!(result, "boolean");
// }

// #[test]
// fn infer_generic_index_value_on_interface() {
//     let lib = r#"
//     interface Foo {
//         [x: number]: <T>(arg: T) => T;
//         bar: boolean;
//     }
//     "#;
//     let mut checker = parse_dts(lib).unwrap();

//     let src = r#"
//     declare let foo: Foo;
//     let id = foo[5];
//     "#;
//     let result = parse(src);
//     let mut prog = match result {
//         Ok(prog) => prog,
//         Err(err) => {
//             println!("err = {:?}", err);
//             panic!("Error parsing expression");
//         }
//     };

//     escalier_old_infer::infer_prog(&mut prog, &mut checker).unwrap();

//     let result = format!("{}", checker.lookup_value("id").unwrap());
//     // NOTE: The type variables aren't normalized.  See comment inside
//     // norm_type() in escalier_old_infer/src/util.rs.
//     assert_eq!(result, "<T>(arg: T) => T | undefined");
// }

// #[test]
// // #[should_panic = "\\\"hello\\\" is an invalid key for object types"]
// fn infer_index_with_incorrect_key_type_on_interface() {
//     let lib = r#"
//     interface Foo {
//         [x: number]: number;
//         bar: boolean;
//     }
//     "#;

//     let src = r#"
//     declare let foo: Foo;
//     let num = foo["hello"];
//     let bool = foo.bar;
//     "#;

//     let error_messages = infer_prog_with_type_error(lib, src);

//     assert_eq!(
//         error_messages,
//         vec!["TypeError::InvalidKey: \"hello\" is not a valid key"]
//     );
// }

// #[test]
// fn instantiating_generic_interfaces() {
//     let lib = r#"
//     interface Foo<T> {
//         bar(x: T): any;
//         baz(x: T): any;
//     }
//     "#;
//     let mut checker = parse_dts(lib).unwrap();

//     let src = r#"
//     declare let foo: Foo<number>;
//     let bar = foo.bar;
//     "#;
//     let result = parse(src);
//     let mut prog = match result {
//         Ok(prog) => prog,
//         Err(err) => {
//             println!("err = {:?}", err);
//             panic!("Error parsing expression");
//         }
//     };

//     escalier_old_infer::infer_prog(&mut prog, &mut checker).unwrap();

//     let result = format!("{}", checker.lookup_value("bar").unwrap());
//     assert_eq!(result, "<A>(x: number) => A");
// }

// #[test]
// fn interface_with_generic_method() {
//     let lib = r#"
//     interface Foo<T> {
//         bar<U>(x: U): U;
//         baz: T;
//     }
//     "#;
//     let mut checker = parse_dts(lib).unwrap();

//     let src = r#"
//     declare let foo: Foo<number>;
//     let bar = foo.bar;
//     "#;
//     let result = parse(src);
//     let mut prog = match result {
//         Ok(prog) => prog,
//         Err(err) => {
//             println!("err = {:?}", err);
//             panic!("Error parsing expression");
//         }
//     };

//     escalier_old_infer::infer_prog(&mut prog, &mut checker).unwrap();

//     let result = format!("{}", checker.lookup_value("bar").unwrap());
//     assert_eq!(result, "<U>(x: U) => U");
// }

// #[test]
// fn merging_generic_interfaces() {
//     let lib = r#"
//     interface Foo<T> {
//         bar(x: T): number;
//     }

//     interface Foo<T> {
//         baz(x: T): string;
//     }
//     "#;
//     let mut checker = parse_dts(lib).unwrap();

//     let src = r#"
//     declare let foo: Foo<number>;
//     "#;
//     let result = parse(src);
//     let mut prog = match result {
//         Ok(prog) => prog,
//         Err(err) => {
//             println!("err = {:?}", err);
//             panic!("Error parsing expression");
//         }
//     };

//     escalier_old_infer::infer_prog(&mut prog, &mut checker).unwrap();

//     let result = format!("{}", checker.lookup_scheme("Foo").unwrap());
//     assert_eq!(
//         result,
//         "<T>{bar(self, x: T): number, baz(self, x: T): string}"
//     );
// }

// #[test]
// fn infer_partial() {
//     let src = r#"
//     type Obj = {a: number, b?: string, mut c: boolean, mut d?: number};
//     type PartialObj = Partial<Obj>;
//     "#;
//     let (_, ctx) = infer_prog(src);
//     let mut checker = Checker::from(ctx);
//     let t = checker.lookup_type("PartialObj").unwrap();
//     let t = checker.expand_type(&t).unwrap();

//     let result = format!("{}", t);
//     assert_eq!(
//         result,
//         "{a?: number, b?: string, mut c?: boolean, mut d?: number}"
//     );
// }

// #[test]
// fn infer_required() {
//     let src = r#"
//     type Obj = {a: number, b?: string, mut c: boolean, mut d?: number};
//     type RequiredObj = Required<Obj>;
//     "#;
//     let (_, ctx) = infer_prog(src);
//     let mut checker = Checker::from(ctx);
//     let t = checker.lookup_type("RequiredObj").unwrap();
//     let t = checker.expand_type(&t).unwrap();

//     let result = format!("{}", t);
//     assert_eq!(
//         result,
//         "{a: number, b: string, mut c: boolean, mut d: number}"
//     );
// }

// #[test]
// fn infer_readonly() {
//     let src = r#"
//     type Obj = {a: number, b?: string, mut c: boolean, mut d?: number};
//     type ReadonlyObj = Readonly<Obj>;
//     "#;
//     let (_, ctx) = infer_prog(src);
//     let mut checker = Checker::from(ctx);
//     let t = checker.lookup_type("ReadonlyObj").unwrap();
//     let t = checker.expand_type(&t).unwrap();

//     let result = format!("{}", t);
//     assert_eq!(result, "{a: number, b?: string, c: boolean, d?: number}");
// }

// #[test]
// fn infer_readonly_with_indexer_only() {
//     let src = r#"
//     type Obj = {[key: string]: boolean};
//     type ReadonlyObj = Readonly<Obj>;
//     "#;
//     let (_, ctx) = infer_prog(src);
//     let mut checker = Checker::from(ctx);
//     let t = checker.lookup_type("ReadonlyObj").unwrap();
//     let t = checker.expand_type(&t).unwrap();

//     let result = format!("{}", t);
//     assert_eq!(result, "{[key: string]: boolean}");
// }

// #[test]
// fn infer_readonly_with_indexer_and_other_properties() {
//     let src = r#"
//     type Obj = {a: number, b?: string, mut c: boolean, mut d?: number, [key: number]: boolean};
//     type ReadonlyObj = Readonly<Obj>;
//     "#;
//     let (_, ctx) = infer_prog(src);
//     let mut checker = Checker::from(ctx);
//     let t = checker.lookup_type("ReadonlyObj").unwrap();
//     let t = checker.expand_type(&t).unwrap();

//     let result = format!("{}", t);
//     assert_eq!(
//         result,
//         "{[key: number]: boolean, a: number, b?: string, c: boolean, d?: number}"
//     );
// }

// #[test]
// fn infer_pick() {
//     let src = r#"
//     type Obj = {a: number, b?: string, mut c: boolean, mut d?: number};
//     type PickObj = Pick<Obj, "a" | "b">;
//     "#;
//     let (_, ctx) = infer_prog(src);
//     let mut checker = Checker::from(ctx);
//     let t = checker.lookup_type("PickObj").unwrap();
//     let t = checker.expand_type(&t).unwrap();

//     let result = format!("{}", t);
//     assert_eq!(result, "{a: number, b?: string}");
// }

// #[test]
// fn infer_prog_using_partial() {
//     let src = r#"
//     type Obj = {a: number, b?: string, mut c: boolean, mut d?: number};
//     type PartialObj = Partial<Obj>;

//     let partial_obj: PartialObj = {b: "hello"};
//     "#;

//     infer_prog(src);
// }

// #[test]
// fn infer_partial_with_getters_and_setters_on_class_instance() {
//     let src = r#"
//     class Foo {
//         get bar(self): number { return 5; }
//         set baz(mut self, value: string) {}
//         qux(): boolean { return true; }
//     }
//     type PartialFoo = Partial<Foo>;

//     type T1 = PartialFoo["bar"];
//     type T2 = PartialFoo["baz"];
//     type T3 = PartialFoo["qux"];
//     "#;

//     let (_, ctx) = infer_prog(src);
//     let mut checker = Checker::from(ctx);

//     let t = checker.lookup_type("T1").unwrap();
//     let t = checker.expand_type(&t).unwrap();
//     let result = format!("{}", t);
//     assert_eq!(result, "number | undefined");

//     let t = checker.lookup_type("T2").unwrap();
//     let t = checker.expand_type(&t).unwrap();
//     let result = format!("{}", t);
//     assert_eq!(result, "string | undefined");

//     let t = checker.lookup_type("T3").unwrap();
//     let t = checker.expand_type(&t).unwrap();
//     let result = format!("{}", t);
//     assert_eq!(result, "() => boolean | undefined"); // should be (() => boolean) | undefined
// }

// #[test]
// fn tuple_mapping() {
//     let src = r#"
//     let tuple = [1, 2, 3];
//     let squares = tuple.map((x) => x * x);
//     let sqr_fn = (x) => x * x;
//     let squares2 = [1, 2, 3].map(sqr_fn);
//     let squares3 = [1, 2, 3].map((x) => x * x);
//     let strings = [1, 2, 3].map((x) => `x = ${x}`);
//     "#;

//     let (_, ctx) = infer_prog(src);

//     let t = ctx.lookup_value("squares").unwrap();
//     assert_eq!(t.to_string(), "mut number[]");
//     let t = ctx.lookup_value("strings").unwrap();
//     assert_eq!(t.to_string(), "mut string[]");
// }

// #[test]
// fn infer_exclude() {
//     let src = r#"
//     type T1 = Exclude<"a" | "b" | "c", "a" | "b">;
//     "#;
//     let (_, ctx) = infer_prog(src);
//     let mut checker = Checker::from(ctx);
//     let t = checker.lookup_type("T1").unwrap();

//     let result = format!("{}", t);
//     assert_eq!(result, "Exclude<\"a\" | \"b\" | \"c\", \"a\" | \"b\">");

//     let t = checker.expand_type(&t).unwrap();
//     let result = format!("{}", t);

//     assert_eq!(result, "\"c\"");
// }

// #[test]
// fn infer_out_of_order_exclude() {
//     let lib = r#"
//     type Exclude<U, T> = T extends U ? never : T;
//     "#;
//     let mut checker = parse_dts(lib).unwrap();

//     let src = r#"
//     type T1 = Exclude<"a" | "b", "a" | "b" | "c">;
//     "#;
//     let result = parse(src);
//     let mut prog = match result {
//         Ok(prog) => prog,
//         Err(err) => {
//             println!("err = {:?}", err);
//             panic!("Error parsing expression");
//         }
//     };
//     escalier_old_infer::infer_prog(&mut prog, &mut checker).unwrap();

//     let t = checker.lookup_type("T1").unwrap();
//     let t = checker.expand_type(&t).unwrap();
//     let result = format!("{}", t);
//     assert_eq!(result, "\"c\"");
// }

// #[test]
// fn infer_omit() {
//     let src = r#"
//     type Obj = {a: number, b?: string, mut c: boolean, mut d?: number};
//     type T1 = Omit<Obj, "b" | "c">;
//     "#;
//     let (_, ctx) = infer_prog(src);
//     let mut checker = Checker::from(ctx);
//     let t = checker.lookup_type("T1").unwrap();

//     let result = format!("{}", t);
//     assert_eq!(result, "Omit<Obj, \"b\" | \"c\">");

//     let t = checker.expand_type(&t).unwrap();
//     let result = format!("{}", t);

//     assert_eq!(result, "{a: number, mut d?: number}");
// }

// #[test]
// fn infer_omit_string() {
//     let src = r#"
//     type T1 = Omit<String, "length">;
//     "#;
//     let (_, ctx) = infer_prog(src);
//     let mut checker = Checker::from(ctx);
//     let t = checker.lookup_type("T1").unwrap();

//     let result = format!("{}", t);
//     assert_eq!(result, "Omit<String, \"length\">");

//     let t = checker.expand_type(&t).unwrap();
//     let result = format!("{}", t);

//     assert!(!result.contains("length: number"));
// }

// #[test]
// fn infer_method_type_with_indexed_access() {
//     let src = r#"
//     type T1 = String["charAt"];
//     "#;
//     let (_, ctx) = infer_prog(src);
//     let mut checker = Checker::from(ctx);
//     let t = checker.lookup_type("T1").unwrap();

//     let t = checker.expand_type(&t).unwrap();
//     let result = format!("{}", t);

//     assert_eq!(result, "(pos: number) => string");
// }

// #[test]
// fn infer_getter_setter_types_with_indexed_access() {
//     let src = r#"
//     class Foo {
//         get bar(self): number { return 5; }
//         set baz(mut self, value: string) {}
//         qux(): boolean { return true; }
//     }
//     type T1 = Foo["bar"];
//     type T2 = Foo["baz"];
//     type T3 = Foo["qux"];
//     "#;
//     let (_, ctx) = infer_prog(src);
//     let mut checker = Checker::from(ctx);

//     let t = checker.lookup_type("T1").unwrap();
//     let t = checker.expand_type(&t).unwrap();
//     let result = format!("{}", t);
//     assert_eq!(result, "number");

//     let t = checker.lookup_type("T2").unwrap();
//     let t = checker.expand_type(&t).unwrap();
//     let result = format!("{}", t);
//     assert_eq!(result, "string");

//     let t = checker.lookup_type("T3").unwrap();
//     let t = checker.expand_type(&t).unwrap();
//     let result = format!("{}", t);
//     assert_eq!(result, "() => boolean");
// }

// #[test]
// fn new_expressions() {
//     let src = r#"
//     let array = new Array(1, 2, 3);
//     "#;

//     let (_, ctx) = infer_prog(src);

//     let t = ctx.lookup_value("array").unwrap();
//     let result = format!("{}", t);
//     assert_eq!(result, "1 | 2 | 3[]");
// }

// #[test]
// fn new_expressions_instantiation_check() {
//     let src = r#"
//     let numbers = new Array(1, 2, 3);
//     let letters = new Array("a", "b", "c");
//     "#;

//     let (_, ctx) = infer_prog(src);

//     let t = ctx.lookup_value("numbers").unwrap();
//     let result = format!("{}", t);
//     assert_eq!(result, "1 | 2 | 3[]");

//     let t = ctx.lookup_value("letters").unwrap();
//     let result = format!("{}", t);
//     assert_eq!(result, r#""a" | "b" | "c"[]"#);
// }

// #[test]
// fn rest_fn() {
//     let src = r#"
//     declare let foo: <T>(...items: mut T[]) => string;
//     let result = foo(1, 2, 3);
//     "#;

//     let (_, ctx) = infer_prog(src);

//     let t = ctx.lookup_value("result").unwrap();
//     let result = format!("{}", t);
//     assert_eq!(result, "string");
// }

// #[test]
// fn infer_infer_type() {
//     let src = r#"
//     type Flatten<Type> = Type extends Array<infer Item> ? Item : Type;

//     let num1: Flatten<Array<number>> = 5;
//     let num2: Flatten<number> = 10;

//     type StringArray = Array<string>;
//     type StringItem = Flatten<StringArray>;
//     let str: StringItem = "hello";

//     type GetReturnType<Type> = Type extends (...args: never[]) => infer Return
//         ? Return
//         : never;

//     let foo = () => 5;
//     type RetType = GetReturnType<typeof foo>;
//     let retVal: RetType = 5;

//     let bar = () => "hello";
//     type RetType = GetReturnType<typeof bar>;
//     let retVal: RetType = "hello";
//     "#;

//     infer_prog(src);
// }

// #[test]
// fn regex_with_named_capture_groups() {
//     let src = r#"
//     let regex = /(?<foo>foo)(?<bar>bar)/;
//     let result = "foobar".match(regex);
//     "#;

//     let (_, ctx) = infer_prog(src);

//     let regex = ctx.lookup_value("regex").unwrap();
//     assert_eq!(
//         regex.to_string(),
//         "RegExp<\"(?<foo>foo)(?<bar>bar)\", \"\">"
//     );

//     let result = ctx.lookup_value("result").unwrap();
//     assert_eq!(
//         result.to_string(),
//         "[string, string, string] & {groups: {foo: string, bar: string}} | null"
//     );
// }

// #[test]
// fn regex_with_g_flag_returns_only_matches() {
//     let src = r#"
//     let regex = /(?<foo>foo)(?<bar>bar)/g;
//     let result = "foobar".match(regex);
//     "#;

//     let (_, ctx) = infer_prog(src);

//     let regex = ctx.lookup_value("regex").unwrap();
//     assert_eq!(
//         regex.to_string(),
//         "RegExp<\"(?<foo>foo)(?<bar>bar)\", \"g\">"
//     );

//     let result = ctx.lookup_value("result").unwrap();
//     assert_eq!(result.to_string(), "null | string[]");
// }
