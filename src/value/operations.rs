use sea_query::{Alias, Expr, SimpleExpr};

use super::{MyTyp, NumTyp, Typed, Value, ValueBuilder};

#[derive(Clone, Copy)]
pub struct Add<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B> Typed for Add<A, B> {
    type Typ = A::Typ;
}
impl<'t, S, A: Value<'t, S>, B: Value<'t, S>> Value<'t, S> for Add<A, B> {
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).add(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Not<T>(pub(crate) T);

impl<T> Typed for Not<T> {
    type Typ = bool;
}
impl<'t, S, T: Value<'t, S>> Value<'t, S> for Not<T> {
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).not()
    }
}

#[derive(Clone, Copy)]
pub struct And<A, B>(pub(crate) A, pub(crate) B);

impl<A, B> Typed for And<A, B> {
    type Typ = bool;
}
impl<'t, S, A: Value<'t, S>, B: Value<'t, S>> Value<'t, S> for And<A, B> {
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).and(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Lt<A, B>(pub(crate) A, pub(crate) B);

impl<A, B> Typed for Lt<A, B> {
    type Typ = bool;
}
impl<'t, S, A: Value<'t, S>, B: Value<'t, S>> Value<'t, S> for Lt<A, B> {
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).lt(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Eq<A, B>(pub(crate) A, pub(crate) B);

impl<A, B> Typed for Eq<A, B> {
    type Typ = bool;
}
impl<'t, S, A: Value<'t, S>, B: Value<'t, S>> Value<'t, S> for Eq<A, B> {
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).eq(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct UnwrapOr<A, B>(pub(crate) A, pub(crate) B);

impl<A, B: Typed> Typed for UnwrapOr<A, B> {
    type Typ = B::Typ;
}
impl<'t, S, A: Value<'t, S>, B: Value<'t, S>> Value<'t, S> for UnwrapOr<A, B> {
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).if_null(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct IsNotNull<A>(pub(crate) A);

impl<A> Typed for IsNotNull<A> {
    type Typ = bool;
}
impl<'t, S, A: Value<'t, S>> Value<'t, S> for IsNotNull<A> {
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).is_not_null()
    }
}

#[derive(Clone, Copy)]
pub struct Assume<A>(pub(crate) A);

impl<T: MyTyp, A: Typed<Typ = Option<T>>> Typed for Assume<A> {
    type Typ = T;
}
impl<'t, S, T: MyTyp, A: Value<'t, S, Typ = Option<T>>> Value<'t, S> for Assume<A> {
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b)
    }
}

#[derive(Clone, Copy)]
pub struct AsFloat<A>(pub(crate) A);

impl<A> Typed for AsFloat<A> {
    type Typ = f64;
}
impl<'t, S, A: Value<'t, S>> Value<'t, S> for AsFloat<A> {
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).cast_as(Alias::new("real"))
    }
}

#[derive(Clone, Copy)]
pub struct Const<A>(pub(crate) A);

impl<A: MyTyp> Typed for Const<A> {
    type Typ = A;
}
impl<'t, S, A: NumTyp> Value<'t, S> for Const<A> {
    fn build_expr(&self, _b: ValueBuilder) -> SimpleExpr {
        SimpleExpr::Constant(self.0.into_value())
    }
}
