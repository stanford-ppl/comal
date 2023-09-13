use core::fmt;
use std::io::Error;
use std::marker::PhantomData;

use dam_rs::templates::ops::ALUOp;
use dam_rs::templates::ops::PipelineRegister;
use dam_rs::types::DAMType;
use dam_rs::types::StaticallySized;
use dam_rs::RegisterALUOp;
use itertools::Chunk;
use itertools::Itertools;
use ndarray::Array;
use ndarray::CowArray;
use ndarray::Dimension;
use ndarray::IntoDimension;
use ndarray::Shape;

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Hash)]
pub enum Token<ValType, StopType> {
    Val(ValType),
    Stop(StopType),
    Empty,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Repsiggen {
    Repeat,
    Stop,
    Done,
}

pub trait Exp {
    fn exp(self) -> Self;
}

#[derive(Clone, PartialEq, Debug)]
pub struct Tensor<'a, ValType: DAMType, Dim: ndarray::Dimension> {
    pub data: ndarray::CowArray<'a, ValType, Dim>,
}

// impl<'a, A, D> FromStr for Tensor<'a, A, D>
// where
//     A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized + num::Zero,
//     D: Dimension,
// {
//     // type Err = Box<dyn std::error::Error>;
//     type Err = Error;

//     fn from_str(s: &str) -> Result<Self, Self::Err> {
//     }
// }

struct PrimitiveType<T: DAMType> {
    marker: PhantomData<T>,
}

trait Adapter<T> {
    fn parse(&self, iter: impl Iterator<Item = String>) -> Vec<T>;
}

impl<T> Adapter<T> for PrimitiveType<T>
where
    T: std::str::FromStr + dam_rs::types::StaticallySized,
{
    fn parse(&self, iter: impl Iterator<Item = String>) -> Vec<T> {
        iter.flat_map(|line| line.parse::<T>()) // ignores Err variant from Result of str.parse
            .collect()
    }
}

impl<'a, A, D> Adapter<Tensor<'a, A, D>> for PrimitiveType<Tensor<'a, A, D>>
where
    A: PartialEq
        + std::fmt::Debug
        + Clone
        + Default
        + Sync
        + Send
        + StaticallySized
        + num::Zero
        + std::str::FromStr,
    D: Dimension,
{
    fn parse(&self, iter: impl Iterator<Item = String>) -> Vec<Tensor<'a, A, D>> {
        let mut out_vec = vec![];
        let chunk_size = 3;
        // type T = impl Iterator<Item = String>;
        (&iter.chunks(chunk_size)).into_iter().for_each(|chunk| {
            out_vec.push(Tensor {
                data: CowArray::from(
                    (chunk
                        .into_iter()
                        .flat_map(|line| line.parse::<A>())
                        .collect()),
                ),
            });
        });
        out_vec
    }
}

impl<'a, A, D> DAMType for Tensor<'a, A, D>
where
    A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized + num::Zero,
    D: Dimension,
{
    fn dam_size(&self) -> usize {
        self.data.dim().into_dimension().size() * A::SIZE
    }
}

impl<'a, A, D> Tensor<'a, A, D>
where
    A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized,
    D: Dimension,
{
    fn size(&self) -> usize {
        self.data.dim().into_dimension().size()
    }
}

impl<'a, A, D> Default for Tensor<'a, A, D>
where
    A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized + num::Zero,
    D: Dimension,
{
    fn default() -> Self {
        Tensor {
            data: CowArray::from(Array::zeros(Shape::from(D::default()))),
        }
    }
}

RegisterALUOp!(ALUExpOp, |(i0), ()| [i0.exp()], T: DAMType + Exp);

impl<ValType: DAMType, StopType: DAMType> Exp for Token<ValType, StopType>
where
    ValType: Exp,
{
    fn exp(self) -> Self {
        match self {
            Token::Val(val) => Token::Val(val.exp()),
            _ => self,
        }
    }
}

impl<T: num::Float> Exp for T {
    fn exp(self) -> Self {
        num::Float::exp(self)
    }
}

// impl<ValType: DAMType, StopType> From<ValType> for Token<ValType, StopType> {
//     fn from(value: ValType) -> Self {
//         Self::Val(value)
//     }
// }

impl<ValType: DAMType, StopType: DAMType> fmt::Debug for Token<ValType, StopType> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::Val(val) => {
                write!(f, "{:#?}", val)
            }
            Token::Stop(tkn) => {
                write!(f, "S{:#?}", tkn)
            }
            Token::Empty => {
                write!(f, "N")
            }
            Token::Done => {
                write!(f, "D")
            }
        }
    }
}

impl<ValType, StopType: core::str::FromStr> TryFrom<&str> for Token<ValType, StopType> {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.starts_with("D") {
            Ok(Self::Done)
        } else if value.starts_with("N") {
            Ok(Self::Empty)
        } else if value.starts_with("S") {
            value[1..].parse().map(Self::Stop).map_err(|_| ())
        } else {
            Err(())
        }
    }
}

impl TryFrom<&str> for Repsiggen {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.starts_with("R") {
            Ok(Self::Repeat)
        } else if value.starts_with("S") {
            Ok(Self::Stop)
        } else if value.starts_with("D") {
            Ok(Self::Done)
        } else {
            Err(())
        }
    }
}

#[macro_export]
macro_rules! token_vec {
    [$toktype: tt; $stoptype: tt; $($val:expr),*] => {
        ({
            let hl = frunk::hlist![$($val),*];
            let mapped = hl.map(
                frunk::poly_fn![
                    |f: &'static str| -> Token<$toktype, $stoptype> {Token::<$toktype, $stoptype>::try_from(f).unwrap()},
                    |v: $toktype| -> Token<$toktype, $stoptype> {Token::<$toktype, $stoptype>::Val(v)},
                    ]
            );
            let result = vec![];
            mapped.foldl(|mut acc: Vec<_>, x| {acc.push(x); acc}, result)
        })
    };
}

#[macro_export]
macro_rules! repsig_vec {
    [$($val:expr),*] => {
        ({
            let mut res = Vec::new();
            $(
                {
                    res.push(Repsiggen::try_from($val).unwrap());
                }
            )*
            res
        })
    };
}

impl<ValType: DAMType, StopType: DAMType> std::ops::Neg for Token<ValType, StopType>
where
    ValType: std::ops::Neg<Output = ValType>,
{
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            Token::Val(val) => Token::Val(val.neg()),
            _ => self,
        }
    }
}

fn tmp() {
    let _ = token_vec![u16; u16; 1, 2, 3, "S0", 4, 5, 6, "S1", "D"];
    let _ = repsig_vec!("R", "R", "S", "D");
}

impl<ValType: Default, StopType: Default> Default for Token<ValType, StopType> {
    fn default() -> Self {
        Token::Val(ValType::default())
    }
}

impl Default for Repsiggen {
    fn default() -> Self {
        Repsiggen::Repeat
    }
}

impl<ValType: DAMType, StopType: DAMType> DAMType for Token<ValType, StopType> {
    fn dam_size(&self) -> usize {
        2 + match self {
            Token::Val(val) => val.dam_size(),
            Token::Stop(stkn) => stkn.dam_size(),
            Token::Empty => 0,
            Token::Done => 0,
        }
    }
}

impl DAMType for Repsiggen {
    fn dam_size(&self) -> usize {
        2 + match self {
            // Not sure exact size beyond 2 bits so using match just in case to update later
            Repsiggen::Repeat => 0,
            Repsiggen::Stop => 0,
            Repsiggen::Done => 0,
        }
    }
}
