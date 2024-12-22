use sea_query::{extension::sqlite::SqliteExpr, Alias, Expr, Keyword, LikeExpr, SimpleExpr};

use super::{IntoColumn, NumTyp, Typed, ValueBuilder};

#[derive(Clone, Copy)]
pub struct Add<A, B>(pub(crate) A, pub(crate) B);

macro_rules! binop {
    ($name:ident) => {
        impl<'t, S, A: IntoColumn<'t, S>, B: IntoColumn<'t, S>> IntoColumn<'t, S> for $name<A, B> {
            type Owned = $name<A::Owned, B::Owned>;

            fn into_owned(self) -> Self::Owned {
                $name(self.0.into_owned(), self.1.into_owned())
            }
        }
    };
}

macro_rules! unop {
    ($name:ident) => {
        impl<'t, S, T: IntoColumn<'t, S>> IntoColumn<'t, S> for $name<T> {
            type Owned = $name<T::Owned>;

            fn into_owned(self) -> Self::Owned {
                $name(self.0.into_owned())
            }
        }
    };
}

impl<A: Typed, B: Typed> Typed for Add<A, B> {
    type Typ = A::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).add(self.1.build_expr(b))
    }
}

binop! {Add}

#[derive(Clone, Copy)]
pub struct And<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B: Typed> Typed for And<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).and(self.1.build_expr(b))
    }
}
binop! {And}

#[derive(Clone, Copy)]
pub struct Or<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B: Typed> Typed for Or<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).or(self.1.build_expr(b))
    }
}
binop! {Or}

#[derive(Clone, Copy)]
pub struct Lt<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B: Typed> Typed for Lt<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).lt(self.1.build_expr(b))
    }
}
binop! {Lt}

#[derive(Clone, Copy)]
pub struct Eq<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B: Typed> Typed for Eq<A, B> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).eq(self.1.build_expr(b))
    }
}
binop! {Eq}

#[derive(Clone, Copy)]
pub struct UnwrapOr<A, B>(pub(crate) A, pub(crate) B);

impl<A: Typed, B: Typed> Typed for UnwrapOr<A, B> {
    type Typ = B::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).if_null(self.1.build_expr(b))
    }
}
binop! {UnwrapOr}

#[derive(Clone, Copy)]
pub struct Not<T>(pub(crate) T);

impl<T: Typed> Typed for Not<T> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b).not()
    }
}
unop! {Not}

#[derive(Clone, Copy)]
pub struct IsNotNull<A>(pub(crate) A);

impl<A: Typed> Typed for IsNotNull<A> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).is_not_null()
    }
}
unop! {IsNotNull}

#[derive(Clone, Copy)]
pub struct AndThen<A, B>(pub(crate) A, pub(crate) B);

// TODO: make this impl stricter? A and B should be Typ=Option
impl<A: Typed, B: Typed> Typed for AndThen<A, B> {
    type Typ = B::Typ;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::case(
            Expr::expr(self.0.build_expr(b)).is_null(),
            SimpleExpr::Keyword(Keyword::Null),
        )
        .finally(self.1.build_expr(b))
        .into()
    }
}
binop! {AndThen}

#[derive(Clone, Copy)]
pub struct Assume<A>(pub(crate) A);

impl<T, A: Typed<Typ = Option<T>>> Typed for Assume<A> {
    type Typ = T;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        self.0.build_expr(b)
    }
}
impl<'t, S, T: IntoColumn<'t, S, Typ = Option<X>>, X> IntoColumn<'t, S> for Assume<T> {
    type Owned = Assume<T::Owned>;
    fn into_owned(self) -> Self::Owned {
        Assume(self.0.into_owned())
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
unop! {AsFloat}

#[derive(Clone)]
pub struct Like<A>(pub(crate) A, pub(crate) String);

impl<A: Typed> Typed for Like<A> {
    type Typ = bool;
    fn build_expr(&self, b: ValueBuilder) -> SimpleExpr {
        Expr::expr(self.0.build_expr(b)).like(LikeExpr::new(&self.1).escape('\\'))
    }
}

impl<'t, S, A: IntoColumn<'t, S>> IntoColumn<'t, S> for Like<A> {
    type Owned = Like<A::Owned>;

    fn into_owned(self) -> Self::Owned {
        Like(self.0.into_owned(), self.1)
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

binop!(Glob);

#[derive(Clone, Copy)]
pub struct Const<A>(pub(crate) A);

impl<A: NumTyp> Typed for Const<A> {
    type Typ = A;
    fn build_expr(&self, _b: ValueBuilder) -> SimpleExpr {
        SimpleExpr::Constant(self.0.into_sea_value())
    }
}
impl<'t, S, A: NumTyp> IntoColumn<'t, S> for Const<A> {
    type Owned = Self;

    fn into_owned(self) -> Self::Owned {
        self
    }
}
