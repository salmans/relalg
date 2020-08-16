use super::{Collector, Expression, ListCollector, Visitor};
use crate::{Tuple, Tuples};
use std::marker::PhantomData;

#[derive(Clone)]
pub struct Diff<T, L, R>
where
    T: Tuple,
    L: Expression<T>,
    R: Expression<T>,
{
    left: L,
    right: R,
    _marker: PhantomData<T>,
}

impl<T, L, R> Diff<T, L, R>
where
    T: Tuple,
    L: Expression<T>,
    R: Expression<T>,
{
    pub fn new(left: &L, right: &R) -> Self {
        Self {
            left: left.clone(),
            right: right.clone(),
            _marker: PhantomData,
        }
    }

    pub fn left(&self) -> &L {
        &self.left
    }

    pub fn right(&self) -> &R {
        &self.right
    }
}

impl<T, L, R> Expression<T> for Diff<T, L, R>
where
    T: Tuple,
    L: Expression<T>,
    R: Expression<T>,
{
    fn visit<V>(&self, visitor: &mut V)
    where
        V: Visitor,
    {
        visitor.visit_diff(&self);
    }

    fn collect<C>(&self, collector: &C) -> anyhow::Result<Tuples<T>>
    where
        C: Collector,
    {
        collector.collect_diff(&self)
    }

    fn collect_list<C>(&self, collector: &C) -> anyhow::Result<Vec<Tuples<T>>>
    where
        C: ListCollector,
    {
        collector.collect_diff(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, Singleton};

    #[test]
    fn test_clone_diff() {
        let mut database = Database::new();
        let r = database.add_relation::<i32>("r");
        let s = database.add_relation::<i32>("s");
        r.insert(vec![1, 2, 3, 6].into(), &database).unwrap();
        s.insert(vec![1, 4, 3, 5].into(), &database).unwrap();
        let u = Diff::new(&r, &s).clone();
        assert_eq!(
            Tuples::<i32>::from(vec![2, 6]),
            database.evaluate(&u).unwrap()
        );
    }

    #[test]
    fn test_evaluate_intersect() {
        {
            let mut database = Database::new();
            let r = database.add_relation::<i32>("r");
            let s = database.add_relation::<i32>("s");
            let u = Diff::new(&r, &s);

            let result = database.evaluate(&u).unwrap();
            assert_eq!(Tuples::<i32>::from(vec![]), result);
        }
        {
            let mut database = Database::new();
            let r = database.add_relation::<i32>("r");
            let s = database.add_relation::<i32>("s");
            r.insert(vec![1, 2, 3].into(), &database).unwrap();
            let u = Diff::new(&r, &s);

            let result = database.evaluate(&u).unwrap();
            assert_eq!(Tuples::<i32>::from(vec![1, 2, 3]), result);
        }
        {
            let mut database = Database::new();
            let r = database.add_relation::<i32>("r");
            let s = database.add_relation::<i32>("s");
            s.insert(vec![4, 5].into(), &database).unwrap();
            let u = Diff::new(&r, &s);

            let result = database.evaluate(&u).unwrap();
            assert_eq!(Tuples::<i32>::from(vec![]), result);
        }

        {
            let database = Database::new();
            let r = Singleton(42);
            let s = Singleton(43);
            let u = Diff::new(&r, &s);

            let result = database.evaluate(&u).unwrap();
            assert_eq!(Tuples::<i32>::from(vec![42]), result);
        }
        {
            let mut database = Database::new();
            let r = database.add_relation::<i32>("r");
            let s = database.add_relation::<i32>("s");
            let u = Diff::new(&r, &s);
            r.insert(vec![1, 2, 3, 4].into(), &database).unwrap();
            s.insert(vec![0, 4, 2, 6].into(), &database).unwrap();

            let result = database.evaluate(&u).unwrap();
            assert_eq!(Tuples::<i32>::from(vec![1, 3]), result);
        }
        {
            let mut database = Database::new();
            let r = database.add_relation::<i32>("r");
            let s = database.add_relation::<i32>("s");
            let t = database.add_relation::<i32>("t");
            let u1 = Diff::new(&r, &s);
            let u2 = Diff::new(&u1, &t);

            r.insert(vec![1, 2, 3, 4, 5].into(), &database).unwrap();
            s.insert(vec![100, 4, 2].into(), &database).unwrap();
            t.insert(vec![1, 2, 4, 100].into(), &database).unwrap();

            let result = database.evaluate(&u2).unwrap();
            assert_eq!(Tuples::<i32>::from(vec![3, 5]), result);
        }
        {
            let mut database = Database::new();
            let mut dummy = Database::new();
            let r = dummy.add_relation::<i32>("r");
            let s = database.add_relation::<i32>("s");
            let u = Diff::new(&r, &s);
            assert!(database.evaluate(&u).is_err());
        }
    }
}