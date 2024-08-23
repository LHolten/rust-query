use ref_cast::RefCast;
use sea_query::BinOper;

use super::{BinOp, ExtraMethods, IsNotNull, UnOp, UnwrapOr, Value};

#[derive(RefCast)]
#[repr(transparent)]
pub struct Integer<T>(T);

impl ExtraMethods for i64 {
    type Dummy<T> = Integer<T>;
}

impl<T: Clone> Integer<T> {
    pub fn lt<'a, R: Value<'a, Typ = i64>>(&self, rhs: R) -> BinOp<bool, T, R> {
        BinOp::new(BinOper::SmallerThan, self.0.clone(), rhs)
    }

    pub fn eq<'a, R: Value<'a, Typ = i64>>(&self, rhs: R) -> BinOp<bool, T, R> {
        BinOp::new(BinOper::Equal, self.0.clone(), rhs)
    }

    pub fn add<'a, R: Value<'a, Typ = i64>>(&self, rhs: R) -> BinOp<i64, T, R> {
        BinOp::new(BinOper::Add, self.0.clone(), rhs)
    }
}

#[derive(RefCast)]
#[repr(transparent)]
pub struct Float<T>(T);

impl ExtraMethods for f64 {
    type Dummy<T> = Float<T>;
}

impl<T: Clone> Float<T> {
    pub fn lt<'a, R: Value<'a, Typ = f64>>(&self, rhs: R) -> BinOp<bool, T, R> {
        BinOp::new(BinOper::SmallerThan, self.0.clone(), rhs)
    }

    pub fn eq<'a, R: Value<'a, Typ = f64>>(&self, rhs: R) -> BinOp<bool, T, R> {
        BinOp::new(BinOper::Equal, self.0.clone(), rhs)
    }

    pub fn add<'a, R: Value<'a, Typ = f64>>(&self, rhs: R) -> BinOp<f64, T, R> {
        BinOp::new(BinOper::Add, self.0.clone(), rhs)
    }
}

#[derive(RefCast)]
#[repr(transparent)]
pub struct Boolean<T>(T);

impl ExtraMethods for bool {
    type Dummy<T> = Boolean<T>;
}

impl<T: Clone> Boolean<T> {
    pub fn eq<'a, R: Value<'a, Typ = bool>>(&self, rhs: R) -> BinOp<bool, T, R> {
        BinOp::new(BinOper::Equal, self.0.clone(), rhs)
    }

    pub fn not(&self) -> UnOp<bool, T> {
        UnOp::new(sea_query::UnOper::Not, self.0.clone())
    }

    pub fn and<'a, R: Value<'a, Typ = bool>>(&self, rhs: R) -> BinOp<bool, T, R> {
        BinOp::new(BinOper::And, self.0.clone(), rhs)
    }
}

#[derive(RefCast)]
#[repr(transparent)]
pub struct Nullable<T>(T);

impl<X> ExtraMethods for Option<X> {
    type Dummy<T> = Nullable<T>;
}

impl<T: Clone> Nullable<T> {
    pub fn unwrap_or<'a, 'b, R: Value<'a>>(&self, rhs: R) -> UnwrapOr<T, R>
    where
        Self: Value<'b, Typ = Option<R::Typ>>,
    {
        UnwrapOr(self.0.clone(), rhs)
    }

    pub fn not_null(&self) -> IsNotNull<T> {
        IsNotNull(self.0.clone())
    }
}

#[derive(RefCast)]
#[repr(transparent)]
pub struct Text<T>(T);

impl ExtraMethods for String {
    type Dummy<T> = Text<T>;
}

impl<T: Clone> Text<T> {
    pub fn eq<'a, R: Value<'a, Typ = String>>(&self, rhs: R) -> BinOp<bool, T, R> {
        BinOp::new(BinOper::Equal, self.0.clone(), rhs)
    }
}
