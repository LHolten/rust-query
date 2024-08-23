use sea_query::{Alias, Expr, SimpleExpr};

use super::{Value, ValueBuilder};

#[derive(Clone, Copy)]
pub struct Add<A, B>(pub(crate) A, pub(crate) B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for Add<A, B> {
    type Typ = A::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).add(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct MyNot<T>(pub(crate) T);

impl<'t, T: Value<'t>> Value<'t> for MyNot<T> {
    type Typ = T::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).not()
    }
}

#[derive(Clone, Copy)]
pub struct And<A, B>(pub(crate) A, pub(crate) B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for And<A, B> {
    type Typ = A::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).and(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Lt<A, B>(pub(crate) A, pub(crate) B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for Lt<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).lt(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Eq<A, B>(pub(crate) A, pub(crate) B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for Eq<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).eq(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct UnwrapOr<A, B>(pub(crate) A, pub(crate) B);

impl<'t, A: Value<'t>, B: Value<'t>> Value<'t> for UnwrapOr<A, B> {
    type Typ = B::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).if_null(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct NotNull<A>(pub(crate) A);

impl<'t, A: Value<'t>> Value<'t> for NotNull<A> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).is_not_null()
    }
}

#[derive(Clone, Copy)]
pub struct Assume<A>(pub(crate) A);

impl<'t, T, A: Value<'t, Typ = Option<T>>> Value<'t> for Assume<A> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b)
    }
}

#[derive(Clone, Copy)]
pub struct AsFloat<A>(pub(crate) A);

impl<'t, A: Value<'t>> Value<'t> for AsFloat<A> {
    type Typ = f64;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).cast_as(Alias::new("real"))
    }
}
