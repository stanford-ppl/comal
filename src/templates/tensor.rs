use std::{
    marker::PhantomData,
    ops::{Add, Mul, Sub},
    str::FromStr,
};

use dam::types::{DAMType, StaticallySized};
use itertools::Itertools;

use ndarray::{
    ArrayBase, ArrayD, CowArray, Dimension, IntoDimension, IxDyn, LinalgScalar, OwnedRepr,
};

#[derive(Clone, PartialEq, Debug)]
pub struct Tensor<'a, ValType: DAMType> {
    pub data: Option<ndarray::CowArray<'a, ValType, IxDyn>>,
}

pub struct PrimitiveType<T: DAMType> {
    pub _marker: PhantomData<T>,
}

impl<T: DAMType> Default for PrimitiveType<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: DAMType> PrimitiveType<T> {
    pub fn new() -> PrimitiveType<T> {
        PrimitiveType::<T> {
            _marker: PhantomData,
        }
    }
}

pub trait Adapter<T> {
    fn parse(
        &self,
        iter: std::iter::Flatten<std::io::Lines<std::io::BufReader<std::fs::File>>>,
        size: Option<usize>,
        blocked: Option<bool>,
    ) -> Vec<T>;
}

impl<T: std::str::FromStr> Adapter<T> for PrimitiveType<T>
where
    T: DAMType,
{
    fn parse(
        &self,
        iter: std::iter::Flatten<std::io::Lines<std::io::BufReader<std::fs::File>>>,
        _size: Option<usize>,
        _blocked: Option<bool>,
    ) -> Vec<T> {
        iter.flat_map(|line| line.parse::<T>()) // ignores Err variant from Result of str.parse
            .collect()
    }
}

impl<'a, A> Adapter<Tensor<'a, A>> for PrimitiveType<Tensor<'a, A>>
where
    A: DAMType + FromStr,
    Tensor<'a, A>: DAMType,
{
    fn parse(
        &self,
        iter: std::iter::Flatten<std::io::Lines<std::io::BufReader<std::fs::File>>>,
        size: Option<usize>,
        blocked: Option<bool>,
    ) -> Vec<Tensor<'a, A>> {
        let mut out_vec = vec![];
        let float_iter = iter.flat_map(|line| line.parse::<A>());

        for chunk in &float_iter.chunks(size.unwrap()) {
            let arr = if blocked.unwrap() {
                ArrayD::from_shape_vec(
                    IxDyn(&[size.unwrap(), size.unwrap()]),
                    chunk.into_iter().collect::<Vec<_>>(),
                )
            } else {
                ArrayD::from_shape_vec(
                    IxDyn(&[size.unwrap()]),
                    chunk.into_iter().collect::<Vec<_>>(),
                )
            };
            out_vec.push(Tensor::<'a, A> {
                data: Some(CowArray::<'a, A, IxDyn>::from(arr.unwrap())),
            });
        }
        out_vec
    }
}

impl<'a, A> Mul for Tensor<'a, A>
where
    A: DAMType,
    CowArray<'a, A, IxDyn>: LinalgScalar,
{
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self {
            data: Some(
                self.data
                    .unwrap()
                    .mul(rhs.data.expect("Attempting to multiply with a None value")),
            ),
        }
    }
}

impl<'a, A: DAMType + std::cmp::PartialEq + PartialOrd> PartialOrd for Tensor<'a, A> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let _test = other.data.to_owned().iter();
        self.data
            .to_owned()
            .unwrap()
            .into_owned()
            .iter()
            .partial_cmp(&other.data.to_owned().unwrap().into_owned())
    }
}

impl<'a, A> Sub for Tensor<'a, A>
where
    A: PartialEq
        + std::fmt::Debug
        + Clone
        + Default
        + Sync
        + Send
        + StaticallySized
        + num::Zero
        + ndarray::RawData,
    CowArray<'a, A, IxDyn>: Sub<Output = CowArray<'a, A, IxDyn>>,
{
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Tensor::<'a, A> {
            // data: self.data.sub(rhs.data),
            data: Some(
                self.data
                    .unwrap()
                    .sub(rhs.data.expect("Attempting to substract with a None value")),
            ),
        }
    }
}

impl<'a, A> Add for Tensor<'a, A>
where
    A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized + num::Zero,
    &'a ArrayBase<OwnedRepr<A>, IxDyn>:
        Add<&'a ArrayBase<OwnedRepr<A>, IxDyn>, Output = ArrayBase<OwnedRepr<A>, IxDyn>>,
    CowArray<'a, A, IxDyn>: Add<Output = CowArray<'a, A, IxDyn>>,
{
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Tensor::<'a, A> {
            data: Some(
                self.data
                    .unwrap()
                    .add(rhs.data.expect("Attempting to substract with a None value")),
            ),
        }
    }
}

impl<'a, A> DAMType for Tensor<'a, A>
where
    A: DAMType + StaticallySized,
    Tensor<'a, A>: Default,
{
    fn dam_size(&self) -> usize {
        self.data
            .as_ref()
            .expect("Attempting to retrieve None tensor")
            .dim()
            .into_dimension()
            .size()
            * A::SIZE
    }
}

impl<'a, A> Tensor<'a, A>
where
    A: PartialEq + std::fmt::Debug + Clone + Default + Sync + Send + StaticallySized,
{
    fn size(&self) -> usize {
        self.data
            .as_ref()
            .expect("Attempting to retrieve None tensor")
            .dim()
            .into_dimension()
            .size()
    }
}

impl<'a, A> Default for Tensor<'a, A>
where
    A: DAMType,
{
    fn default() -> Self {
        Tensor::<'a, A> { data: None }
    }
}
