use itertools::join;
use std::fmt;

use crate::types::TFnParam;
use crate::types::{Type, TypeParam};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TCallable {
    pub params: Vec<TFnParam>,
    pub ret: Box<Type>,
    pub type_params: Vec<TypeParam>,
}

impl fmt::Display for TCallable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Self {
            params,
            ret,
            type_params,
        } = self;
        if type_params.is_empty() {
            write!(f, "({}) => {}", join(params, ", "), ret)
        } else {
            let type_params = type_params.iter().map(|tp| {
                let TypeParam {
                    name,
                    constraint,
                    default: _, // TODO
                } = tp;
                match constraint {
                    Some(constraint) => format!("{name} extends {constraint}"),
                    None => name.to_string(),
                }
            });
            write!(
                f,
                "<{}>({}) => {}",
                join(type_params, ", "),
                join(params, ", "),
                ret
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TMethod {
    pub name: TPropKey,
    pub mutating: bool,
    pub params: Vec<TFnParam>,
    pub ret: Box<Type>,
    pub type_params: Vec<TypeParam>,
}

impl fmt::Display for TMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Self {
            name,
            mutating,
            params,
            ret,
            type_params,
        } = self;
        if *mutating {
            write!(f, "mut ")?;
        }
        write!(f, "{name}")?;
        if !type_params.is_empty() {
            let type_params = type_params.iter().map(|tp| {
                let TypeParam {
                    name,
                    constraint,
                    default: _, // TODO
                } = tp;
                match constraint {
                    Some(constraint) => format!("{name} extends {constraint}"),
                    None => name.to_string(),
                }
            });
            write!(f, "<{}>", join(type_params, ", "))?;
        }
        write!(f, "({}): {}", join(params, ", "), ret)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TGetter {
    pub name: TPropKey,
    pub ret: Box<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TSetter {
    pub name: TPropKey,
    pub param: TFnParam,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TObjElem {
    Call(TCallable),
    Constructor(TCallable),
    Method(TMethod),
    Getter(TGetter),
    Setter(TSetter),
    Index(TIndex),
    Prop(TProp),
    // RestSpread - we can use this instead of converting {a, ...x} to {a} & tvar
}

impl fmt::Display for TObjElem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TObjElem::Call(lam) => write!(f, "{lam}"),
            TObjElem::Constructor(lam) => write!(f, "new {lam}"),
            TObjElem::Method(method) => write!(f, "{method}"),
            TObjElem::Getter(_) => todo!(),
            TObjElem::Setter(_) => todo!(),
            TObjElem::Index(index) => write!(f, "{index}"),
            TObjElem::Prop(prop) => write!(f, "{prop}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TIndexKey {
    pub name: String,
    pub t: Box<Type>,
}

impl fmt::Display for TIndexKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Self { name, t } = self;
        write!(f, "{name}: {t}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TIndex {
    pub key: TIndexKey,
    pub mutable: bool,
    pub t: Type,
}

impl fmt::Display for TIndex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Self { key, mutable, t } = self;
        match mutable {
            false => write!(f, "[{key}]: {t}"),
            true => write!(f, "mut [{key}]: {t}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TPropKey {
    StringKey(String),
    NumberKey(String),
}

impl fmt::Display for TPropKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TPropKey::StringKey(key) => write!(f, "{key}"),
            TPropKey::NumberKey(key) => write!(f, "{key}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TProp {
    pub name: TPropKey,
    pub optional: bool,
    pub mutable: bool,
    pub t: Type,
}

impl fmt::Display for TProp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Self {
            name,
            optional,
            mutable,
            t,
        } = self;
        match (optional, mutable) {
            (false, false) => write!(f, "{name}: {t}"),
            (true, false) => write!(f, "{name}?: {t}"),
            (false, true) => write!(f, "mut {name}: {t}"),
            (true, true) => write!(f, "mut {name}?: {t}"),
        }
    }
}
