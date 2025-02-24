use sea_query::{Alias, Expr, Keyword, LikeExpr, SimpleExpr, extension::sqlite::SqliteExpr};

use super::{NumTyp, Typed, ValueBuilder};

#[derive(Clone, Copy)]
pub struct Add<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B: Typed> Typed for Add<A, B> {
    type Typ = A::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).add(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct And<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B: Typed> Typed for And<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).and(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Or<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B: Typed> Typed for Or<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).or(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Lt<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B: Typed> Typed for Lt<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).lt(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Eq<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B: Typed> Typed for Eq<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).eq(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct UnwrapOr<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B: Typed> Typed for UnwrapOr<A, B> {
    type Typ = B::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).if_null(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Not<T>(pub(crate) T);

impl<T: Typed> Typed for Not<T> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).not()
    }
}

#[derive(Clone, Copy)]
pub struct IsNotNull<A>(pub(crate) A);

impl<A: Typed> Typed for IsNotNull<A> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).is_not_null()
    }
}

#[derive(Clone, Copy)]
/// Return null if `A` is `true` else `B`
pub struct NullIf<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed<Typ = bool>, B: Typed<Typ = Option<T>>, T> Typed for NullIf<A, B> {
    type Typ = B::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::case(self.0.build_expr(b), SimpleExpr::Keyword(Keyword::Null))
            .finally(self.1.build_expr(b))
            .into()
    }
}

#[derive(Clone, Copy)]
pub struct Assume<A>(pub(crate) A);

impl<T: 'static, A: Typed<Typ = Option<T>>> Typed for Assume<A> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b)
    }
}

#[derive(Clone, Copy)]
pub struct AsFloat<A>(pub(crate) A);

impl<A: Typed> Typed for AsFloat<A> {
    type Typ = f64;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).cast_as(Alias::new("real"))
    }
}

#[derive(Clone)]
pub struct Like<A>(pub(crate) A, pub(crate) String);

impl<A: Typed> Typed for Like<A> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).like(LikeExpr::new(&self.1).escape('\\'))
    }
}

#[derive(Clone)]
pub struct Glob<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B: Typed> Typed for Glob<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).glob(self.1.build_expr(b))
    }
}

#[derive(Clone, Copy)]
pub struct Const<A>(pub(crate) A);

impl<A: NumTyp> Typed for Const<A> {
    type Typ = A;
    fn build_expr(&self, _b: ValueBuilder) -> SimpleExpr {
        SimpleExpr::Constant(self.0.into_sea_value())
    }
}
