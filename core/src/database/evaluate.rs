use super::{elements::Elements, Database, Tuples};
use crate::{
    expression::*,
    tools::{diff_helper, intersect_helper, join_helper, project_helper},
    Tuple,
};
use anyhow::Result;

pub(crate) struct Incremental<'d>(pub &'d Database);

impl<'d> Collector for Incremental<'d> {
    fn collect_singleton<T>(&self, _: &Singleton<T>) -> Result<Tuples<T>>
    where
        T: Tuple,
    {
        Ok(Vec::new().into())
    }

    fn collect_relation<T>(&self, relation: &Relation<T>) -> Result<Tuples<T>>
    where
        T: Tuple,
    {
        let table = self.0.relation_instance(relation)?;
        Ok(table.recent.borrow().clone())
    }

    fn collect_select<T, E>(&self, select: &Select<T, E>) -> Result<Tuples<T>>
    where
        T: Tuple,
        E: Expression<T>,
    {
        let mut result = Vec::new();
        let recent = select.expression().collect(self)?;
        let predicate = &mut (*select.predicate().borrow_mut());
        for tuple in &recent[..] {
            if predicate(tuple) {
                result.push(tuple.clone());
            }
        }
        Ok(result.into())
    }

    fn collect_union<T, L, R>(&self, union: &Union<T, L, R>) -> Result<Tuples<T>>
    where
        T: Tuple,
        L: Expression<T>,
        R: Expression<T>,
    {
        let mut result = Vec::new();

        let left_recent = union.left().collect(self)?;
        let right_recent = union.right().collect(self)?;

        project_helper(&left_recent, |t| result.push(t.clone()));
        project_helper(&right_recent, |t| result.push(t.clone()));

        Ok(result.into())
    }

    fn collect_intersect<T, L, R>(&self, intersect: &Intersect<T, L, R>) -> Result<Tuples<T>>
    where
        T: Tuple,
        L: Expression<T>,
        R: Expression<T>,
    {
        let mut result = Vec::new();
        let incremental = Incremental(self.0);

        let left_recent = intersect.left().collect(self)?;
        let right_recent = intersect.right().collect(self)?;

        let left_stable = intersect.left().collect_list(&incremental)?;
        let right_stable = intersect.right().collect_list(&incremental)?;

        for batch in left_stable.iter() {
            intersect_helper(&batch, &right_recent, &mut result)
        }

        for batch in right_stable.iter() {
            intersect_helper(&left_recent, &batch, &mut result)
        }

        intersect_helper(&left_recent, &right_recent, &mut result);
        Ok(result.into())
    }

    fn collect_diff<T, L, R>(&self, diff: &Diff<T, L, R>) -> Result<Tuples<T>>
    where
        T: Tuple,
        L: Expression<T>,
        R: Expression<T>,
    {
        let mut result = Vec::new();
        let incremental = Incremental(self.0);

        let left_recent = diff.left().collect(self)?;
        let left_stable = diff.left().collect_list(&incremental)?;
        let right_stable = diff.right().collect_list(&incremental)?;

        for batch in left_stable.iter() {
            diff_helper(&batch, &right_stable, &mut result)
        }

        diff_helper(&left_recent, &right_stable, &mut result);
        Ok(result.into())
    }

    fn collect_project<S, T, E>(&self, project: &Project<S, T, E>) -> Result<Tuples<T>>
    where
        T: Tuple,
        S: Tuple,
        E: Expression<S>,
    {
        let mut result = Vec::new();
        let recent = project.expression().collect(self)?;
        let mapper = &mut (*project.mapper().borrow_mut());
        project_helper(&recent, |t| result.push(mapper(t)));
        Ok(result.into())
    }

    fn collect_join<K, L, R, Left, Right, T>(
        &self,
        join: &Join<K, L, R, Left, Right, T>,
    ) -> Result<Tuples<T>>
    where
        K: Tuple,
        L: Tuple,
        R: Tuple,
        T: Tuple,
        Left: Expression<(K, L)>,
        Right: Expression<(K, R)>,
    {
        let mut result = Vec::new();
        let incremental = Incremental(self.0);

        let left_recent = join.left().collect(self)?;
        let right_recent = join.right().collect(self)?;

        let left_stable = join.left().collect_list(&incremental)?;
        let right_stable = join.right().collect_list(&incremental)?;

        let mapper = &mut (*join.mapper().borrow_mut());

        for batch in left_stable.iter() {
            join_helper(&batch, &right_recent, |k, v1, v2| {
                result.push(mapper(k, v1, v2))
            });
        }

        for batch in right_stable.iter() {
            join_helper(&left_recent, &batch, |k, v1, v2| {
                result.push(mapper(k, v1, v2))
            });
        }

        join_helper(&left_recent, &right_recent, |k, v1, v2| {
            result.push(mapper(k, v1, v2))
        });

        Ok(result.into())
    }

    fn collect_view<T, E>(&self, view: &View<T, E>) -> Result<Tuples<T>>
    where
        T: Tuple,
        E: Expression<T> + 'static,
    {
        let table = self.0.view_instance(view)?;
        Ok(table.recent.borrow().clone())
    }
}

impl<'d> ListCollector for Incremental<'d> {
    fn collect_singleton<T>(&self, singleton: &Singleton<T>) -> Result<Vec<Tuples<T>>>
    where
        T: Tuple,
    {
        Ok(vec![vec![singleton.0.clone()].into()])
    }

    fn collect_relation<T>(&self, relation: &Relation<T>) -> Result<Vec<Tuples<T>>>
    where
        T: Tuple,
    {
        let mut result = Vec::<Tuples<T>>::new();
        let table = self.0.relation_instance(&relation)?;
        for batch in table.stable.borrow().iter() {
            result.push(batch.clone());
        }
        Ok(result)
    }

    fn collect_select<T, E>(&self, select: &Select<T, E>) -> Result<Vec<Tuples<T>>>
    where
        T: Tuple,
        E: Expression<T>,
    {
        let mut result = Vec::<Tuples<T>>::new();
        let stable = select.expression().collect_list(self)?;
        let predicate = &mut (*select.predicate().borrow_mut());
        for batch in stable.iter() {
            let mut tuples = Vec::new();
            for tuple in &batch[..] {
                if predicate(tuple) {
                    tuples.push(tuple.clone());
                }
            }
            result.push(tuples.into());
        }
        Ok(result)
    }

    fn collect_union<T, L, R>(&self, union: &Union<T, L, R>) -> Result<Vec<Tuples<T>>>
    where
        T: Tuple,
        L: Expression<T>,
        R: Expression<T>,
    {
        let mut result = Vec::<Tuples<T>>::new();
        let left_stable = union.left().collect_list(self)?;
        let right_stable = union.right().collect_list(self)?;

        for batch in left_stable.iter() {
            let mut tuples = Vec::new();
            project_helper(&batch, |t| tuples.push(t.clone()));
            result.push(tuples.into());
        }
        for batch in right_stable.iter() {
            let mut tuples = Vec::new();
            project_helper(&batch, |t| tuples.push(t.clone()));
            result.push(tuples.into());
        }

        Ok(result)
    }

    fn collect_intersect<T, L, R>(&self, intersect: &Intersect<T, L, R>) -> Result<Vec<Tuples<T>>>
    where
        T: Tuple,
        L: Expression<T>,
        R: Expression<T>,
    {
        let mut result = Vec::<Tuples<T>>::new();
        let left = intersect.left().collect_list(self)?;
        let right = intersect.right().collect_list(self)?;

        for left_batch in left.iter() {
            let mut tuples = Vec::new();
            for right_batch in right.iter() {
                intersect_helper(&left_batch, &right_batch, &mut tuples);
            }
            result.push(tuples.into());
        }
        Ok(result)
    }

    fn collect_diff<T, L, R>(&self, diff: &Diff<T, L, R>) -> Result<Vec<Tuples<T>>>
    where
        T: Tuple,
        L: Expression<T>,
        R: Expression<T>,
    {
        let mut result = Vec::<Tuples<T>>::new();
        let left = diff.left().collect_list(self)?;
        let right = diff.right().collect_list(self)?;

        for batch in left.iter() {
            let mut tuples = Vec::new();
            diff_helper(&batch, &right, &mut tuples);
            result.push(tuples.into());
        }
        Ok(result)
    }

    fn collect_project<S, T, E>(&self, project: &Project<S, T, E>) -> Result<Vec<Tuples<T>>>
    where
        T: Tuple,
        S: Tuple,
        E: Expression<S>,
    {
        let mut result = Vec::<Tuples<T>>::new();
        let stable = project.expression().collect_list(self)?;
        let mapper = &mut (*project.mapper().borrow_mut());
        for batch in stable.iter() {
            let mut tuples = Vec::new();
            project_helper(&batch, |t| tuples.push(mapper(t)));
            result.push(tuples.into());
        }
        Ok(result)
    }

    fn collect_join<K, L, R, Left, Right, T>(
        &self,
        join: &Join<K, L, R, Left, Right, T>,
    ) -> Result<Vec<Tuples<T>>>
    where
        K: Tuple,
        L: Tuple,
        R: Tuple,
        T: Tuple,
        Left: Expression<(K, L)>,
        Right: Expression<(K, R)>,
    {
        let mut result = Vec::<Tuples<T>>::new();
        let left = join.left().collect_list(self)?;
        let right = join.right().collect_list(self)?;

        let mapper = &mut (*join.mapper().borrow_mut());
        for left_batch in left.iter() {
            let mut tuples = Vec::new();
            for right_batch in right.iter() {
                join_helper(&left_batch, &right_batch, |k, v1, v2| {
                    tuples.push(mapper(k, v1, v2))
                });
            }
            result.push(tuples.into());
        }
        Ok(result)
    }

    fn collect_view<T, E>(&self, view: &View<T, E>) -> Result<Vec<Tuples<T>>>
    where
        T: Tuple,
        E: Expression<T> + 'static,
    {
        let mut result = Vec::<Tuples<T>>::new();
        let table = self.0.view_instance(&view)?;
        for batch in table.stable.borrow().iter() {
            result.push(batch.clone());
        }
        Ok(result)
    }
}

pub(crate) struct Evaluator<'d>(pub &'d Database);

impl<'d> Collector for Evaluator<'d> {
    fn collect_singleton<T>(&self, singleton: &Singleton<T>) -> Result<Tuples<T>>
    where
        T: Tuple,
    {
        Ok(vec![singleton.0.clone()].into())
    }

    fn collect_relation<T>(&self, relation: &Relation<T>) -> Result<Tuples<T>>
    where
        T: Tuple,
    {
        self.0.recalculate_relation(&relation.name)?;
        let table = self.0.relation_instance(&relation)?;
        assert!(table.recent.borrow().is_empty());
        assert!(table.to_add.borrow().is_empty());

        let incremental = Incremental(self.0);

        let mut result = relation.collect(&incremental)?;
        for batch in relation.collect_list(&incremental)? {
            result = result.merge(batch);
        }

        Ok(result)
    }

    fn collect_select<T, E>(&self, select: &Select<T, E>) -> Result<Tuples<T>>
    where
        T: Tuple,
        E: Expression<T>,
    {
        let mut elements = crate::database::elements::Elements::new();
        select.visit(&mut elements);

        for r in elements.relations() {
            self.0.recalculate_relation(&r)?;
        }

        for r in elements.views() {
            self.0.recalculate_view(&r)?;
        }

        let incremental = Incremental(self.0);

        let mut result = select.collect(&incremental)?;
        for batch in select.collect_list(&incremental)? {
            result = result.merge(batch);
        }
        Ok(result)
    }

    fn collect_union<T, L, R>(&self, union: &Union<T, L, R>) -> Result<Tuples<T>>
    where
        T: Tuple,
        L: Expression<T>,
        R: Expression<T>,
    {
        let mut elements = crate::database::elements::Elements::new();
        union.visit(&mut elements);

        for r in elements.relations() {
            self.0.recalculate_relation(&r)?;
        }

        for r in elements.views() {
            self.0.recalculate_view(&r)?;
        }

        let incremental = Incremental(self.0);

        let mut result = union.collect(&incremental)?;
        for batch in union.collect_list(&incremental)? {
            result = result.merge(batch);
        }
        Ok(result)
    }

    fn collect_intersect<T, L, R>(&self, intersect: &Intersect<T, L, R>) -> Result<Tuples<T>>
    where
        T: Tuple,
        L: Expression<T>,
        R: Expression<T>,
    {
        let mut elements = Elements::new();
        intersect.visit(&mut elements);

        for r in elements.relations() {
            self.0.recalculate_relation(&r)?;
        }

        for r in elements.views() {
            self.0.recalculate_view(&r)?;
        }

        let incremental = Incremental(self.0);

        let mut result = intersect.collect(&incremental)?;
        for batch in intersect.collect_list(&incremental)? {
            result = result.merge(batch);
        }

        Ok(result)
    }

    fn collect_diff<T, L, R>(&self, diff: &Diff<T, L, R>) -> Result<Tuples<T>>
    where
        T: Tuple,
        L: Expression<T>,
        R: Expression<T>,
    {
        let mut elements = Elements::new();
        diff.visit(&mut elements);

        for r in elements.relations() {
            self.0.recalculate_relation(&r)?;
        }

        for r in elements.views() {
            self.0.recalculate_view(&r)?;
        }

        let incremental = Incremental(self.0);

        let mut result = diff.collect(&incremental)?;
        for batch in diff.collect_list(&incremental)? {
            result = result.merge(batch);
        }

        Ok(result)
    }

    fn collect_project<S, T, E>(&self, project: &Project<S, T, E>) -> Result<Tuples<T>>
    where
        T: Tuple,
        S: Tuple,
        E: Expression<S>,
    {
        let mut elements = crate::database::elements::Elements::new();
        project.visit(&mut elements);

        for r in elements.relations() {
            self.0.recalculate_relation(&r)?;
        }

        for r in elements.views() {
            self.0.recalculate_view(&r)?;
        }

        let incremental = Incremental(self.0);

        let mut result = project.collect(&incremental)?;
        for batch in project.collect_list(&incremental)? {
            result = result.merge(batch);
        }
        Ok(result)
    }

    fn collect_join<K, L, R, Left, Right, T>(
        &self,
        join: &Join<K, L, R, Left, Right, T>,
    ) -> Result<Tuples<T>>
    where
        K: Tuple,
        L: Tuple,
        R: Tuple,
        T: Tuple,
        Left: Expression<(K, L)>,
        Right: Expression<(K, R)>,
    {
        let mut elements = Elements::new();
        join.visit(&mut elements);

        for r in elements.relations() {
            self.0.recalculate_relation(&r)?;
        }

        for r in elements.views() {
            self.0.recalculate_view(&r)?;
        }

        let incremental = Incremental(self.0);

        let mut result = join.collect(&incremental)?;
        for batch in join.collect_list(&incremental)? {
            result = result.merge(batch);
        }

        Ok(result)
    }

    fn collect_view<T, E>(&self, view: &View<T, E>) -> Result<Tuples<T>>
    where
        T: Tuple,
        E: Expression<T> + 'static,
    {
        self.0.recalculate_view(&view.reference)?;
        let table = self.0.view_instance(view)?;
        assert!(table.recent.borrow().is_empty());
        assert!(table.to_add.borrow().is_empty());

        let incremental = Incremental(self.0);

        let mut result = view.collect(&incremental)?;
        for batch in view.collect_list(&incremental)? {
            result = result.merge(batch);
        }

        Ok(result)
    }
}