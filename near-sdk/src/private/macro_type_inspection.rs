//! This module implements a variation of [autoref specialization]. Autoref specialization
//! is a way to work around the lack of trait specialization in stable Rust by abusing deref coercion
//! (aka autoref).
//!
//! We use specialization to be able to handle methods returning `Result<_, _>` differently
//! from other serializable types, with our framework automatically detecting them (no need
//! for an explicit attribute).
//!
//! [autoref specialization]: https://github.com/dtolnay/case-studies/blob/master/autoref-specialization/README.md

use std::marker::PhantomData;

use borsh::{schema::BorshSchemaContainer, BorshSchema};
use schemars::{schema::RootSchema, schema_for, JsonSchema};

pub trait SerializationFormat {
    type SchemaObject;
}

pub struct Json;
impl SerializationFormat for Json {
    type SchemaObject = RootSchema;
}

pub struct Borsh;
impl SerializationFormat for Borsh {
    type SchemaObject = BorshSchemaContainer;
}

trait SerializableWith<S: SerializationFormat> {
    fn schema() -> S::SchemaObject;
}

impl<T: JsonSchema> SerializableWith<Json> for T {
    fn schema() -> RootSchema {
        schema_for!(T)
    }
}

impl<T: BorshSchema> SerializableWith<Borsh> for T {
    fn schema() -> BorshSchemaContainer {
        T::schema_container()
    }
}

pub trait ContractReturn<S: SerializationFormat> {
    // The method return type as specified by the user of the framework.
    type Input;
    // The `Ok` type in the normalized `Result<Ok, _>`.
    type Okay;

    // This should be treated as an associated function. The only reason
    // the `self` receiver is present is for us to be able to abuse method
    // resolution to emulate specialization.
    //
    // The only reason the `_serialization_format` parameter is here is
    // so that we can disambiguate the `S` type parameter in scenarios
    // where we abuse deref coercion.
    fn schema(self, _serialization_format: S) -> S::SchemaObject;

    // The `self` receiver is only here as an anchor - we abuse method resolution
    // (deref coercion) to emulate specialization. The real receiver is the `ret`
    // parameter.
    //
    // The only reason the `_serialization_format` parameter is here is
    // so that we can disambiguate the `S` type parameter in scenarios
    // where we abuse deref coercion.
    fn normalize_return(
        self,
        _serialization_format: S,
        ret: Self::Input,
    ) -> Result<Self::Okay, &'static str>;
}

impl<S: SerializationFormat, T: SerializableWith<S>> ContractReturn<S> for PhantomData<T> {
    type Input = T;
    type Okay = T;

    fn schema(self, _serialization_format: S) -> S::SchemaObject {
        T::schema()
    }

    fn normalize_return(
        self,
        _serialization_format: S,
        ret: Self::Input,
    ) -> Result<Self::Okay, &'static str> {
        Ok(ret)
    }
}

impl<S: SerializationFormat, T: SerializableWith<S>> ContractReturn<S>
    for &PhantomData<Result<T, &'static str>>
{
    type Input = Result<T, &'static str>;
    type Okay = T;

    fn schema(self, _serialization_format: S) -> S::SchemaObject {
        T::schema()
    }

    fn normalize_return(
        self,
        _serialization_format: S,
        ret: Self::Input,
    ) -> Result<Self::Okay, &'static str> {
        ret
    }
}

#[cfg(test)]
#[allow(clippy::needless_borrow)]
mod tests {
    use super::*;

    // These cases look silly in handwritten code, but the generalized form of these expressions
    // is useful for macro expansions.

    #[test]
    fn basic_type() {
        assert_eq!(Ok(55), (&PhantomData::<u32>).normalize_return(Json, 55));
    }

    #[test]
    fn result_type() {
        assert_eq!(
            Ok(55),
            (&PhantomData::<Result<u32, &'static str>>).normalize_return(Json, Ok(55))
        );
    }

    #[test]
    fn basic_json_schema() {
        assert_eq!(schema_for!(u32), (&PhantomData::<u32>).schema(Json));
    }

    #[test]
    fn result_json_schema() {
        assert_eq!(schema_for!(u32), (&PhantomData::<Result<u32, &'static str>>).schema(Json));
    }

    #[test]
    fn basic_borsh_schema() {
        assert_eq!(u32::schema_container(), (&PhantomData::<u32>).schema(Borsh));
    }

    #[test]
    fn result_borsh_schema() {
        assert_eq!(
            u32::schema_container(),
            (&PhantomData::<Result<u32, &'static str>>).schema(Borsh)
        );
    }
}
