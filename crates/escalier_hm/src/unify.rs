use defaultmap::*;
use generational_arena::{Arena, Index};
use itertools::Itertools;
use std::collections::{BTreeSet, HashMap};

use escalier_ast::{BindingIdent, Expr, Literal as Lit, Span};

use crate::checker::Checker;
use crate::context::*;
use crate::errors::*;
use crate::infer::check_mutability;
use crate::types::*;

impl Checker {
    /// Unify the two types t1 and t2.
    ///
    /// Makes the types t1 and t2 the same.
    ///
    /// Args:
    ///     t1: The first type to be made equivalent (subtype)
    ///     t2: The second type to be be equivalent (supertype)
    ///
    /// Returns:
    ///     None
    ///
    /// Raises:
    ///     InferenceError: Raised if the types cannot be unified.
    pub fn unify(&mut self, ctx: &Context, t1: Index, t2: Index) -> Result<(), Errors> {
        let a = self.prune(t1);
        let b = self.prune(t2);

        // TODO: only expand if unification fails since it's expensive
        let a = self.expand(ctx, a)?;
        let b = self.expand(ctx, b)?;

        let a_t = self.arena[a].clone();
        let b_t = self.arena[b].clone();

        match (&a_t.kind, &b_t.kind) {
            (TypeKind::Variable(_), _) => self.bind(ctx, a, b),
            (_, TypeKind::Variable(_)) => self.bind(ctx, b, a),

            // Wildcards are always unifiable
            (TypeKind::Wildcard, _) => Ok(()),
            (_, TypeKind::Wildcard) => Ok(()),

            (TypeKind::Keyword(kw1), TypeKind::Keyword(kw2)) => {
                if kw1 == kw2 {
                    Ok(())
                } else {
                    Err(Errors::InferenceError(format!(
                        "type mismatch: {} != {}",
                        a_t.as_string(&self.arena),
                        b_t.as_string(&self.arena)
                    )))
                }
            }

            (_, TypeKind::Keyword(Keyword::Unknown)) => {
                // All types are assignable to `unknown`
                Ok(())
            }

            (TypeKind::Union(union), _) => {
                // All types in the union must be subtypes of t2
                for t in union.types.iter() {
                    self.unify(ctx, *t, b)?;
                }
                Ok(())
            }
            (_, TypeKind::Union(union)) => {
                // If t1 is a subtype of any of the types in the union, then it is a
                // subtype of the union.
                for t2 in union.types.iter() {
                    if self.unify(ctx, a, *t2).is_ok() {
                        return Ok(());
                    }
                }

                Err(Errors::InferenceError(format!(
                    "type mismatch: unify({}, {}) failed",
                    a_t.as_string(&self.arena),
                    b_t.as_string(&self.arena)
                )))
            }
            (TypeKind::Tuple(tuple1), TypeKind::Tuple(tuple2)) => {
                'outer: {
                    if tuple1.types.len() < tuple2.types.len() {
                        // If there's a rest pattern in tuple1, then it can unify
                        // with the reamining elements of tuple2.
                        if let Some(last) = tuple1.types.last() {
                            if let TypeKind::Rest(_) = self.arena[*last].kind {
                                break 'outer;
                            }
                        }

                        return Err(Errors::InferenceError(format!(
                            "Expected tuple of length {}, got tuple of length {}",
                            tuple2.types.len(),
                            tuple1.types.len()
                        )));
                    }
                }

                for (i, (p, q)) in tuple1.types.iter().zip(tuple2.types.iter()).enumerate() {
                    // let q_t = arena[*q];
                    match (&self.arena[*p].kind, &self.arena[*q].kind) {
                        (TypeKind::Rest(_), TypeKind::Rest(_)) => {
                            return Err(Errors::InferenceError(
                                "Can't unify two rest elements".to_string(),
                            ))
                        }
                        (TypeKind::Rest(_), _) => {
                            let rest_q = new_tuple_type(&mut self.arena, &tuple2.types[i..]);
                            self.unify(ctx, *p, rest_q)?;
                        }
                        (_, TypeKind::Rest(_)) => {
                            let rest_p = new_tuple_type(&mut self.arena, &tuple1.types[i..]);
                            self.unify(ctx, rest_p, *q)?;
                        }
                        (_, _) => self.unify(ctx, *p, *q)?,
                    }
                }
                Ok(())
            }
            (TypeKind::Tuple(tuple), TypeKind::Constructor(array)) if array.name == "Array" => {
                let q = array.types[0];
                for p in &tuple.types {
                    match &self.arena[*p].kind {
                        TypeKind::Constructor(Constructor { name, types }) if name == "Array" => {
                            self.unify(ctx, types[0], q)?;
                        }
                        TypeKind::Rest(_) => self.unify(ctx, *p, b)?,
                        _ => self.unify(ctx, *p, q)?,
                    }
                }
                Ok(())
            }
            (TypeKind::Constructor(array), TypeKind::Tuple(tuple)) if array.name == "Array" => {
                let p = array.types[0];
                for q in &tuple.types {
                    let undefined = new_keyword(&mut self.arena, Keyword::Undefined);
                    let p_or_undefined = new_union_type(&mut self.arena, &[p, undefined]);

                    match &self.arena[*q].kind {
                        TypeKind::Rest(_) => self.unify(ctx, a, *q)?,
                        _ => self.unify(ctx, p_or_undefined, *q)?,
                    }
                }
                Ok(())
            }
            (TypeKind::Rest(rest), TypeKind::Constructor(array)) if (array.name == "Array") => {
                self.unify(ctx, rest.arg, b)
            }
            (TypeKind::Rest(rest), TypeKind::Tuple(_)) => self.unify(ctx, rest.arg, b),
            (TypeKind::Constructor(array), TypeKind::Rest(rest)) if (array.name == "Array") => {
                self.unify(ctx, a, rest.arg)
            }
            (TypeKind::Tuple(_), TypeKind::Rest(rest)) => self.unify(ctx, a, rest.arg),
            (TypeKind::Constructor(con_a), TypeKind::Constructor(con_b)) => {
                // TODO: support type constructors with optional and default type params
                if con_a.name != con_b.name || con_a.types.len() != con_b.types.len() {
                    return Err(Errors::InferenceError(format!(
                        "type mismatch: {} != {}",
                        a_t.as_string(&self.arena),
                        b_t.as_string(&self.arena),
                    )));
                }
                for (p, q) in con_a.types.iter().zip(con_b.types.iter()) {
                    self.unify(ctx, *p, *q)?;
                }
                Ok(())
            }
            (TypeKind::Function(func_a), TypeKind::Function(func_b)) => {
                // Is this the right place to instantiate the function types?
                let func_a = instantiate_func(&mut self.arena, func_a, None)?;
                let func_b = instantiate_func(&mut self.arena, func_b, None)?;

                let mut params_a = func_a.params;
                let mut params_b = func_b.params;

                if let Some(param) = params_a.get(0) {
                    if param.is_self() {
                        params_a.remove(0);
                    }
                }

                if let Some(param) = params_b.get(0) {
                    if param.is_self() {
                        params_b.remove(0);
                    }
                }

                let mut rest_a = None;
                let mut rest_b = None;

                for param in &params_a {
                    if let TPat::Rest(rest) = &param.pattern {
                        if rest_a.is_some() {
                            return Err(Errors::InferenceError(
                                "multiple rest params in function".to_string(),
                            ));
                        }
                        rest_a = Some((rest, param.t));
                    }
                }

                for param in &params_b {
                    if let TPat::Rest(rest) = &param.pattern {
                        if rest_b.is_some() {
                            return Err(Errors::InferenceError(
                                "multiple rest params in function".to_string(),
                            ));
                        }
                        rest_b = Some((rest, param.t));
                    }
                }

                // TODO: remove leading `self` or `mut self` param before proceding

                let min_params_a = params_a.len() - rest_a.is_some() as usize;
                let min_params_b = params_b.len() - rest_b.is_some() as usize;

                if min_params_a > min_params_b {
                    if let Some(rest_b) = rest_b {
                        for i in 0..min_params_b {
                            let p = &params_a[i];
                            let q = &params_b[i];
                            // NOTE: We reverse the order of the params here because func_a
                            // should be able to accept any params that func_b can accept,
                            // its params may be more lenient.
                            self.unify(ctx, q.t, p.t)?;
                        }

                        let mut remaining_args_a = vec![];

                        for p in &params_a[min_params_b..] {
                            let arg = match &p.pattern {
                                TPat::Rest(_) => match &self.arena[p.t].kind {
                                    TypeKind::Tuple(tuple) => {
                                        for t in &tuple.types {
                                            remaining_args_a.push(*t);
                                        }
                                        continue;
                                    }
                                    TypeKind::Constructor(array) if array.name == "Array" => {
                                        new_rest_type(&mut self.arena, p.t)
                                    }
                                    TypeKind::Constructor(_) => todo!(),
                                    _ => {
                                        return Err(Errors::InferenceError(format!(
                                            "rest param must be an array or tuple, got {}",
                                            self.print_type(&p.t)
                                        )));
                                    }
                                },
                                _ => p.t,
                            };

                            remaining_args_a.push(arg);
                        }

                        let remaining_args_a = new_tuple_type(&mut self.arena, &remaining_args_a);

                        // NOTE: We reverse the order of the params here because func_a
                        // should be able to accept any params that func_b can accept,
                        // its params may be more lenient.
                        self.unify(ctx, rest_b.1, remaining_args_a)?;

                        self.unify(ctx, func_a.ret, func_b.ret)?;

                        return Ok(());
                    }

                    return Err(Errors::InferenceError(format!(
                        "{} is not a subtype of {} since it requires more params",
                        a_t.as_string(&self.arena),
                        b_t.as_string(&self.arena),
                    )));
                }

                for i in 0..min_params_a {
                    let p = &params_a[i];
                    let q = &params_b[i];
                    // NOTE: We reverse the order of the params here because func_a
                    // should be able to accept any params that func_b can accept,
                    // its params may be more lenient.
                    self.unify(ctx, q.t, p.t)?;
                }

                if let Some(rest_a) = rest_a {
                    for q in params_b.iter().take(min_params_b).skip(min_params_a) {
                        // NOTE: We reverse the order of the params here because func_a
                        // should be able to accept any params that func_b can accept,
                        // its params may be more lenient.
                        self.unify(ctx, q.t, rest_a.1)?;
                    }

                    if let Some(rest_b) = rest_b {
                        // NOTE: We reverse the order of the params here because func_a
                        // should be able to accept any params that func_b can accept,
                        // its params may be more lenient.
                        self.unify(ctx, rest_b.1, rest_a.1)?;
                    }
                }

                self.unify(ctx, func_a.ret, func_b.ret)?;

                let never = new_keyword(&mut self.arena, Keyword::Never);
                let throws_a = func_a.throws.unwrap_or(never);
                let throws_b = func_b.throws.unwrap_or(never);

                self.unify(ctx, throws_a, throws_b)?;

                Ok(())
            }
            (TypeKind::Literal(lit1), TypeKind::Literal(lit2)) => {
                let equal = match (&lit1, &lit2) {
                    (Lit::Boolean(value1), Lit::Boolean(value2)) => value1 == value2,
                    (Lit::Number(value1), Lit::Number(value2)) => value1 == value2,
                    (Lit::String(value1), Lit::String(value2)) => value1 == value2,
                    _ => false,
                };
                if !equal {
                    return Err(Errors::InferenceError(format!(
                        "type mismatch: {} != {}",
                        a_t.as_string(&self.arena),
                        b_t.as_string(&self.arena),
                    )));
                }
                Ok(())
            }
            (TypeKind::Literal(Lit::Number(_)), TypeKind::Primitive(Primitive::Number)) => Ok(()),
            (TypeKind::Literal(Lit::String(_)), TypeKind::Primitive(Primitive::String)) => Ok(()),
            (TypeKind::Literal(Lit::Boolean(_)), TypeKind::Primitive(Primitive::Boolean)) => Ok(()),
            (TypeKind::Primitive(prim1), TypeKind::Primitive(prim2)) => match (prim1, prim2) {
                (Primitive::Number, Primitive::Number) => Ok(()),
                (Primitive::String, Primitive::String) => Ok(()),
                (Primitive::Boolean, Primitive::Boolean) => Ok(()),
                (Primitive::Symbol, Primitive::Symbol) => Ok(()),
                _ => Err(Errors::InferenceError(format!(
                    "type mismatch: {} != {}",
                    a_t.as_string(&self.arena),
                    b_t.as_string(&self.arena),
                ))),
            },
            (TypeKind::Object(object1), TypeKind::Object(object2)) => {
                // object1 must have atleast as the same properties as object2
                // This is pretty inefficient... we should have some way of hashing
                // each object element so that we can look them.  The problem comes
                // in with functions where different signatures can expand to be the
                // same.  Do these kinds of checks is going to be really slow.
                // We could also try bucketing the different object element types
                // to reduce the size of the n^2.

                // NOTES:
                // - we don't bother unifying setters because they aren't available
                //   on immutable objects (setters need to be unified inside of
                //   unify_mut() below)
                // - we unify all of the other named elements all at once because
                //   a property could be a function and we want that to unify with
                //   a method of the same name
                // - we should also unify indexers with other named values since
                //   they can be accessed by name as well but are optional

                let mut calls_1: Vec<&TCallable> = vec![];
                let mut constructors_1: Vec<&TCallable> = vec![];
                let mut mapped_1: Vec<&MappedType> = vec![];

                let mut calls_2: Vec<&TCallable> = vec![];
                let mut constructors_2: Vec<&TCallable> = vec![];
                let mut mapped_2: Vec<&MappedType> = vec![];

                let named_props_1: HashMap<_, _> = object1
                    .elems
                    .iter()
                    .filter_map(|elem| match elem {
                        TObjElem::Call(_) => None,
                        TObjElem::Constructor(_) => None,
                        TObjElem::Mapped(_) => None,
                        TObjElem::Prop(prop) => {
                            // TODO: handle getters/setters properly
                            Some((prop.name.to_string(), prop))
                        }
                    })
                    .collect();

                let named_props_2: HashMap<_, _> = object2
                    .elems
                    .iter()
                    .filter_map(|elem| match elem {
                        TObjElem::Call(_) => None,
                        TObjElem::Constructor(_) => None,
                        TObjElem::Mapped(_) => None,
                        TObjElem::Prop(prop) => {
                            // TODO: handle getters/setters properly
                            Some((prop.name.to_string(), prop))
                        }
                    })
                    .collect();

                // object1 must have at least as the same named elements as object2
                // TODO: handle the case where object1 has an indexer that covers
                // some of the named elements of object2
                for (name, prop_2) in &named_props_2 {
                    match named_props_1.get(name) {
                        Some(prop_1) => {
                            let t1 = prop_1.get_type(&mut self.arena);
                            let t2 = prop_2.get_type(&mut self.arena);
                            self.unify(ctx, t1, t2)?;
                        }
                        None => {
                            return Err(Errors::InferenceError(format!(
                                "'{}' is missing in {}",
                                name,
                                a_t.as_string(&self.arena),
                            )));
                        }
                    }
                }

                for prop1 in &object1.elems {
                    match prop1 {
                        TObjElem::Call(call) => calls_1.push(call),
                        TObjElem::Constructor(constructor) => constructors_1.push(constructor),
                        TObjElem::Mapped(mapped) => mapped_1.push(mapped),
                        _ => (),
                    }
                }

                for prop2 in &object2.elems {
                    match prop2 {
                        TObjElem::Call(call) => calls_2.push(call),
                        TObjElem::Constructor(constructor) => constructors_2.push(constructor),
                        TObjElem::Mapped(mapped) => mapped_2.push(mapped),
                        _ => (),
                    }
                }

                match mapped_2.len() {
                    0 => (),
                    1 => {
                        match mapped_1.len() {
                            0 => {
                                for (_, prop_1) in named_props_1 {
                                    let undefined =
                                        new_keyword(&mut self.arena, Keyword::Undefined);
                                    let t1 = prop_1.get_type(&mut self.arena);
                                    let t2 = new_union_type(
                                        &mut self.arena,
                                        &[mapped_2[0].value, undefined],
                                    );
                                    self.unify(ctx, t1, t2)?;
                                }
                            }
                            1 => {
                                self.unify(ctx, mapped_1[0].value, mapped_2[0].value)?;
                                // NOTE: the order is reverse here because object1
                                // has to have at least the same keys as object2,
                                // but it can have more.
                                // TODO: lookup source instead of key... we only need
                                // to look at key when it's not using the source or if
                                // it's modifying the source.

                                let mut mapping: HashMap<String, Index> = HashMap::new();
                                mapping.insert(mapped_1[0].target.to_owned(), mapped_1[0].source);
                                let mapped_1_key =
                                    instantiate_scheme(&mut self.arena, mapped_1[0].key, &mapping);

                                let mut mapping: HashMap<String, Index> = HashMap::new();
                                mapping.insert(mapped_2[0].target.to_owned(), mapped_2[0].source);
                                let mapped_2_key =
                                    instantiate_scheme(&mut self.arena, mapped_2[0].key, &mapping);

                                self.unify(ctx, mapped_2_key, mapped_1_key)?;
                            }
                            _ => {
                                return Err(Errors::InferenceError(format!(
                                    "{} has multiple indexers",
                                    a_t.as_string(&self.arena)
                                )))
                            }
                        }
                    }
                    _ => {
                        return Err(Errors::InferenceError(format!(
                            "{} has multiple indexers",
                            b_t.as_string(&self.arena)
                        )))
                    }
                }

                // TODO:
                // - call (all calls in object1 must cover the calls in object2)
                // - constructor (all constructors in object1 must cover the
                //   constructors in object2)
                Ok(())
            }
            (TypeKind::Object(object1), TypeKind::Intersection(intersection)) => {
                let obj_types: Vec<_> = intersection
                    .types
                    .iter()
                    .filter(|t| matches!(self.arena[**t].kind, TypeKind::Object(_)))
                    .cloned()
                    .collect();
                let rest_types: Vec<_> = intersection
                    .types
                    .iter()
                    .filter(|t| matches!(self.arena[**t].kind, TypeKind::Variable(_)))
                    .cloned()
                    .collect();
                // TODO: check for other variants, if there are we should error

                let obj_type = simplify_intersection(&mut self.arena, &obj_types);

                match rest_types.len() {
                    0 => self.unify(ctx, t1, obj_type),
                    1 => {
                        let all_obj_elems = match &self.arena[obj_type].kind {
                            TypeKind::Object(obj) => obj.elems.to_owned(),
                            _ => vec![],
                        };

                        let (obj_elems, rest_elems): (Vec<_>, Vec<_>) =
                            object1.elems.iter().cloned().partition(|e| {
                                all_obj_elems.iter().any(|oe| match (oe, e) {
                                    // What to do about Call signatures?
                                    // (TObjElem::Call(_), TObjElem::Call(_)) => todo!(),
                                    (TObjElem::Prop(op), TObjElem::Prop(p)) => op.name == p.name,
                                    _ => false,
                                })
                            });

                        let new_obj_type = new_object_type(&mut self.arena, &obj_elems);
                        self.unify(ctx, new_obj_type, obj_type)?;

                        let new_rest_type = new_object_type(&mut self.arena, &rest_elems);
                        self.unify(ctx, new_rest_type, rest_types[0])?;

                        Ok(())
                    }
                    _ => Err(Errors::InferenceError(
                        "Inference is undecidable".to_string(),
                    )),
                }
            }
            (TypeKind::Intersection(intersection), TypeKind::Object(object2)) => {
                let obj_types: Vec<_> = intersection
                    .types
                    .iter()
                    .filter(|t| matches!(self.arena[**t].kind, TypeKind::Object(_)))
                    .cloned()
                    .collect();
                let rest_types: Vec<_> = intersection
                    .types
                    .iter()
                    .filter(|t| matches!(self.arena[**t].kind, TypeKind::Variable(_)))
                    .cloned()
                    .collect();

                let obj_type = simplify_intersection(&mut self.arena, &obj_types);

                match rest_types.len() {
                    0 => self.unify(ctx, t1, obj_type),
                    1 => {
                        let all_obj_elems = match &self.arena[obj_type].kind {
                            TypeKind::Object(obj) => obj.elems.to_owned(),
                            _ => vec![],
                        };

                        let (obj_elems, rest_elems): (Vec<_>, Vec<_>) =
                            object2.elems.iter().cloned().partition(|e| {
                                all_obj_elems.iter().any(|oe| match (oe, e) {
                                    // What to do about Call signatures?
                                    // (TObjElem::Call(_), TObjElem::Call(_)) => todo!(),
                                    (TObjElem::Prop(op), TObjElem::Prop(p)) => op.name == p.name,
                                    _ => false,
                                })
                            });

                        let new_obj_type = new_object_type(&mut self.arena, &obj_elems);
                        self.unify(ctx, obj_type, new_obj_type)?;

                        let new_rest_type = new_object_type(&mut self.arena, &rest_elems);
                        self.unify(ctx, rest_types[0], new_rest_type)?;

                        Ok(())
                    }
                    _ => Err(Errors::InferenceError(
                        "Inference is undecidable".to_string(),
                    )),
                }
            }
            _ => Err(Errors::InferenceError(format!(
                "type mismatch: unify({}, {}) failed",
                a_t.as_string(&self.arena),
                b_t.as_string(&self.arena)
            ))),
        }
    }

    pub fn unify_mut(&mut self, ctx: &Context, t1: Index, t2: Index) -> Result<(), Errors> {
        let t1 = self.prune(t1);
        let t2 = self.prune(t2);

        // TODO: only expand if unification fails since it's expensive
        let t1 = self.expand(ctx, t1)?;
        let t2 = self.expand(ctx, t2)?;

        let t1_t = self.arena.get(t1).unwrap();
        let t2_t = self.arena.get(t2).unwrap();

        if t1_t.equals(t2_t, &self.arena) {
            Ok(())
        } else {
            Err(Errors::InferenceError(format!(
                "unify_mut: {} != {}",
                self.print_type(&t1),
                self.print_type(&t2),
            )))
        }
    }

    // This function unifies and infers the return type of a function call.
    pub fn unify_call(
        &mut self,
        ctx: &mut Context,
        args: &mut [Expr],
        type_args: Option<&[Index]>,
        t2: Index,
    ) -> Result<(Index, Option<Index>), Errors> {
        let ret_type = new_var_type(&mut self.arena, None);
        let mut maybe_throws_type: Option<Index> = None;
        // let throws_type = new_var_type(arena, None);

        let b = self.prune(t2);
        let b_t = self.arena.get(b).unwrap().clone();

        match b_t.kind {
            TypeKind::Variable(_) => {
                let arg_types: Vec<FuncParam> = args
                    .iter_mut()
                    .enumerate()
                    .map(|(i, arg)| {
                        let t = self.infer_expression(arg, ctx)?;
                        let param = FuncParam {
                            pattern: TPat::Ident(BindingIdent {
                                name: format!("arg{i}"),
                                mutable: false,
                                // loc: DUMMY_LOC,
                                span: Span { start: 0, end: 0 },
                            }),
                            // name: format!("arg{i}"),
                            t,
                            optional: false,
                        };
                        Ok(param)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let call_type = self.new_func_type(&arg_types, ret_type, &None, None);
                self.bind(ctx, b, call_type)?
            }
            TypeKind::Union(Union { types }) => {
                let mut ret_types = vec![];
                let mut throws_types = vec![];
                for t in types.iter() {
                    let (ret_type, throws_type) = self.unify_call(ctx, args, type_args, *t)?;
                    ret_types.push(ret_type);
                    if let Some(throws_type) = throws_type {
                        throws_types.push(throws_type);
                    }
                }

                let ret = new_union_type(
                    &mut self.arena,
                    &ret_types.into_iter().unique().collect_vec(),
                );
                let throws = new_union_type(
                    &mut self.arena,
                    &throws_types.into_iter().unique().collect_vec(),
                );

                let throws = match &self.arena[throws].kind {
                    TypeKind::Keyword(Keyword::Never) => None,
                    _ => Some(throws),
                };

                return Ok((ret, throws));
            }
            TypeKind::Intersection(Intersection { types }) => {
                for t in types.iter() {
                    // TODO: if there are multiple overloads that unify, pick the
                    // best one.
                    let result = self.unify_call(ctx, args, type_args, *t);
                    match result {
                        Ok(ret_type) => return Ok(ret_type),
                        Err(_) => continue,
                    }
                }
                return Err(Errors::InferenceError(
                    "no valid overload for args".to_string(),
                ));
            }
            TypeKind::Tuple(_) => {
                return Err(Errors::InferenceError("tuple is not callable".to_string()))
            }
            TypeKind::Constructor(Constructor {
                name,
                types: type_args,
            }) => match ctx.schemes.get(&name) {
                Some(scheme) => {
                    let mut mapping: HashMap<String, Index> = HashMap::new();
                    if let Some(type_params) = &scheme.type_params {
                        for (param, arg) in type_params.iter().zip(type_args.iter()) {
                            mapping.insert(param.name.clone(), arg.to_owned());
                        }
                    }

                    let t = instantiate_scheme(&mut self.arena, scheme.t, &mapping);
                    let type_args = if type_args.is_empty() {
                        None
                    } else {
                        Some(type_args.as_slice())
                    };

                    return self.unify_call(ctx, args, type_args, t);
                }
                None => {
                    panic!("Couldn't find scheme for {name:#?}");
                }
            },
            TypeKind::Literal(lit) => {
                return Err(Errors::InferenceError(format!(
                    "literal {lit:#?} is not callable"
                )));
            }
            TypeKind::Primitive(primitive) => {
                return Err(Errors::InferenceError(format!(
                    "Primitive {primitive:#?} is not callable"
                )));
            }
            TypeKind::Keyword(keyword) => {
                return Err(Errors::InferenceError(format!("{keyword} is not callable")))
            }
            TypeKind::Object(_) => {
                // TODO: check if the object has a callbale signature
                return Err(Errors::InferenceError("object is not callable".to_string()));
            }
            TypeKind::Rest(_) => {
                return Err(Errors::InferenceError("rest is not callable".to_string()));
            }
            TypeKind::Function(func) => {
                let func = if func.type_params.is_some() {
                    instantiate_func(&mut self.arena, &func, type_args)?
                } else {
                    func
                };

                if args.len() < func.params.len() {
                    return Err(Errors::InferenceError(format!(
                        "too few arguments to function: expected {}, got {}",
                        func.params.len(),
                        args.len()
                    )));
                }

                let arg_types = args
                    .iter_mut()
                    .map(|arg| {
                        // TODO: handle spreads
                        let t = self.infer_expression(arg, ctx)?;
                        Ok((arg, t))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                for ((arg, p), param) in arg_types.iter().zip(func.params.iter()) {
                    match check_mutability(ctx, &param.pattern, arg)? {
                        true => self.unify_mut(ctx, *p, param.t)?,
                        false => self.unify(ctx, *p, param.t)?,
                    };
                }

                self.unify(ctx, ret_type, func.ret)?;

                if let Some(throws) = func.throws {
                    let throws_type = new_var_type(&mut self.arena, None);
                    self.unify(ctx, throws_type, throws)?;

                    let throws_type = self.prune(throws_type);
                    maybe_throws_type = match &self.arena[throws_type].kind {
                        TypeKind::Keyword(Keyword::Never) => None,
                        _ => Some(throws_type),
                    };
                }
            }
            TypeKind::KeyOf(KeyOf { t }) => {
                return Err(Errors::InferenceError(format!(
                    "keyof {} is not callable",
                    self.print_type(&t)
                )));
            }
            TypeKind::IndexedAccess(IndexedAccess { obj, index }) => {
                let t = self.get_prop(ctx, obj, index)?;
                self.unify_call(ctx, args, type_args, t)?;
            }
            TypeKind::Conditional(Conditional {
                check,
                extends,
                true_type,
                false_type,
            }) => {
                match self.unify(ctx, check, extends) {
                    Ok(_) => self.unify_call(ctx, args, type_args, true_type)?,
                    Err(_) => self.unify_call(ctx, args, type_args, false_type)?,
                };
            }
            TypeKind::Infer(Infer { name }) => {
                return Err(Errors::InferenceError(format!(
                    "infer {name} is not callable",
                )));
            }
            TypeKind::Wildcard => {
                return Err(Errors::InferenceError("_ is not callable".to_string()));
            }
            TypeKind::Binary(BinaryT {
                op: _,
                left: _,
                right: _,
            }) => todo!(),
        }

        // We need to prune the return type, because it might be a type variable.
        let ret_type = self.prune(ret_type);

        Ok((ret_type, maybe_throws_type))
    }

    fn bind(&mut self, ctx: &Context, a: Index, b: Index) -> Result<(), Errors> {
        // eprint!("bind(");
        // eprint!("{:#?}", arena[a].as_string(arena));
        // if let Some(provenance) = &arena[a].provenance {
        //     eprint!(" : {:#?}", provenance);
        // }
        // eprint!(", {:#?}", arena[b].as_string(arena));
        // if let Some(provenance) = &arena[b].provenance {
        //     eprint!(" : {:#?}", provenance);
        // }
        // eprintln!(")");

        if a != b {
            if self.occurs_in_type(a, b) {
                return Err(Errors::InferenceError("recursive unification".to_string()));
            }

            match self.arena.get_mut(a) {
                Some(t) => match &mut t.kind {
                    TypeKind::Variable(avar) => {
                        avar.instance = Some(b);
                        if let Some(constraint) = avar.constraint {
                            self.unify(ctx, b, constraint)?;
                        }
                    }
                    _ => {
                        unimplemented!("bind not implemented for {:#?}", t.kind);
                    }
                },
                None => todo!(),
            }
        }
        Ok(())
    }

    fn expand(&mut self, ctx: &Context, a: Index) -> Result<Index, Errors> {
        let a_t = self.arena[a].clone();

        match &a_t.kind {
            TypeKind::Constructor(Constructor { name, .. }) if name == "Array" => Ok(a),
            TypeKind::Constructor(Constructor { name, .. }) if name == "Promise" => Ok(a),
            _ => self.expand_type(ctx, a),
        }
    }
}

// TODO: handle optional properties correctly
// Maybe we can have a function that will canonicalize objects by converting
// `x: T | undefined` to `x?: T`
pub fn simplify_intersection(arena: &mut Arena<Type>, in_types: &[Index]) -> Index {
    let obj_types: Vec<_> = in_types
        .iter()
        .filter_map(|t| match &arena[*t].kind {
            TypeKind::Object(elems) => Some(elems),
            _ => None,
        })
        .collect();

    // The use of HashSet<Type> here is to avoid duplicate types
    let mut props_map: DefaultHashMap<String, BTreeSet<Index>> = defaulthashmap!();
    for obj in obj_types {
        for elem in &obj.elems {
            match elem {
                // What do we do with Call and Index signatures
                TObjElem::Call(_) => todo!(),
                TObjElem::Constructor(_) => todo!(),
                TObjElem::Mapped(_) => todo!(),
                TObjElem::Prop(prop) => {
                    let key = match &prop.name {
                        TPropKey::StringKey(key) => key.to_owned(),
                        TPropKey::NumberKey(key) => key.to_owned(),
                    };
                    props_map[key].insert(prop.t);
                }
            }
        }
    }

    let mut elems: Vec<TObjElem> = props_map
        .iter()
        .map(|(name, types)| {
            let types: Vec<_> = types.iter().cloned().collect();
            let t: Index = if types.len() == 1 {
                types[0]
            } else {
                // TODO: handle getter/setters correctly
                new_intersection_type(arena, &types)
                // checker.from_type_kind(TypeKind::Intersection(types))
            };
            TObjElem::Prop(TProp {
                name: TPropKey::StringKey(name.to_owned()),
                modifier: None,
                // TODO: determine this field from all of the TProps with
                // the same name.  This should only be optional if all of
                // the TProps with the current name are optional.
                optional: false,
                mutable: false,
                t,
            })
        })
        .collect();
    // How do we sort call and index signatures?
    elems.sort_by_key(|elem| match elem {
        TObjElem::Call(_) => todo!(),
        TObjElem::Constructor(_) => todo!(),
        TObjElem::Mapped(_) => todo!(),
        TObjElem::Prop(prop) => prop.name.clone(),
    }); // ensure a stable order

    let mut not_obj_types: Vec<_> = in_types
        .iter()
        .filter(|t| !matches!(&arena[**t].kind, TypeKind::Object(_)))
        .cloned()
        .collect();

    let mut out_types = vec![];
    out_types.append(&mut not_obj_types);
    if !elems.is_empty() {
        out_types.push(new_object_type(arena, &elems));
    }
    // TODO: figure out a consistent way to sort types
    // out_types.sort_by_key(|t| t.id); // ensure a stable order

    if out_types.len() == 1 {
        out_types[0]
    } else {
        new_intersection_type(arena, &out_types)
    }
}
