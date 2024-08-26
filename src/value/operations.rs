use sea_query::{Alias, Expr, SimpleExpr};

use super::{NoParam, Value, ValueBuilder};

#[derive(Clone, Copy)]
pub struct Add<A, B>(pub(crate) A, pub(crate) B);

impl<A, B> NoParam for Add<A, B> {}
impl<'t, S, A: Value<'t, S>, B: Value<'t, S>> Value<'t, S> for Add<A, B> {
    type Typ = A::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).add(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Not<T>(pub(crate) T);

impl<T> NoParam for Not<T> {}
impl<'t, S, T: Value<'t, S>> Value<'t, S> for Not<T> {
    type Typ = T::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).not()
    }
}

#[derive(Clone, Copy)]
pub struct And<A, B>(pub(crate) A, pub(crate) B);

impl<A, B> NoParam for And<A, B> {}
impl<'t, S, A: Value<'t, S>, B: Value<'t, S>> Value<'t, S> for And<A, B> {
    type Typ = A::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).and(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Lt<A, B>(pub(crate) A, pub(crate) B);

impl<A, B> NoParam for Lt<A, B> {}
impl<'t, S, A: Value<'t, S>, B: Value<'t, S>> Value<'t, S> for Lt<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).lt(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Eq<A, B>(pub(crate) A, pub(crate) B);

impl<A, B> NoParam for Eq<A, B> {}
impl<'t, S, A: Value<'t, S>, B: Value<'t, S>> Value<'t, S> for Eq<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).eq(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct UnwrapOr<A, B>(pub(crate) A, pub(crate) B);

impl<A, B> NoParam for UnwrapOr<A, B> {}
impl<'t, S, A: Value<'t, S>, B: Value<'t, S>> Value<'t, S> for UnwrapOr<A, B> {
    type Typ = B::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).if_null(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct NotNull<A>(pub(crate) A);

impl<A> NoParam for NotNull<A> {}
impl<'t, S, A: Value<'t, S>> Value<'t, S> for NotNull<A> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).is_not_null()
    }
}

#[derive(Clone, Copy)]
pub struct Assume<A>(pub(crate) A);

impl<A> NoParam for Assume<A> {}
impl<'t, S, T, A: Value<'t, S, Typ = Option<T>>> Value<'t, S> for Assume<A> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b)
    }
}

#[derive(Clone, Copy)]
pub struct AsFloat<A>(pub(crate) A);

impl<A> NoParam for AsFloat<A> {}
impl<'t, S, A: Value<'t, S>> Value<'t, S> for AsFloat<A> {
    type Typ = f64;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).cast_as(Alias::new("real"))
    }
}
