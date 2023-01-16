// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! Column expression

use std::any::Any;
use std::sync::Arc;

use arrow::{
    array::StructArray,
    datatypes::{DataType, Schema},
    record_batch::RecordBatch,
};
use arrow_schema::Field;

use crate::physical_expr::down_cast_any_ref;
use crate::{AnalysisContext, PhysicalExpr};
use datafusion_common::{DataFusionError, Result};
use datafusion_expr::ColumnarValue;

/// Represents the column at a given index in a RecordBatch
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct Column {
    name: String,
    index_path: Vec<usize>,
}

impl Column {
    /// Create a new column expression
    pub fn new(name: &str, index: usize) -> Self {
        Self {
            name: name.to_owned(),
            index_path: vec![index],
        }
    }

    /// Create a new column with a path pointing to a nested field (Struct or List)
    pub fn new_with_path(name: &str, index_path: Vec<usize>) -> Self {
        Column {
            name: name.to_owned(),
            index_path,
        }
    }

    /// Create a new column expression based on column name and schema
    pub fn new_with_schema(name: &str, schema: &Schema) -> Result<Self> {
        Ok(Column::new(name, schema.index_of(name)?))
    }

    /// Get the column name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the column index
    pub fn index(&self) -> usize {
        self.index_path[0]
    }
}

impl std::fmt::Display for Column {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}@{}",
            self.name,
            self.index_path
                .iter()
                .map(|index| index.to_string())
                .collect::<Vec<String>>()
                .join(".")
        )
    }
}

impl PhysicalExpr for Column {
    /// Return a reference to Any that can be used for downcasting
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Get the data type of this expression, given the schema of the input
    fn data_type(&self, input_schema: &Schema) -> Result<DataType> {
        Ok(self.field(input_schema)?.data_type().clone())
    }

    /// Decide whehter this expression is nullable, given the schema of the input
    fn nullable(&self, input_schema: &Schema) -> Result<bool> {
        Ok(self.field(input_schema)?.is_nullable())
    }

    /// Evaluate the expression
    fn evaluate(&self, batch: &RecordBatch) -> Result<ColumnarValue> {
        let mut index_iter = self.index_path.iter();
        let mut array = batch
            .column(
                *index_iter
                    .next()
                    .ok_or(self.bounds_error(&batch.schema()))?,
            )
            .clone();
        for index in index_iter {
            array = match array.data_type() {
                DataType::Struct(_) => array
                    .as_any()
                    .downcast_ref::<StructArray>()
                    .ok_or(self.bounds_error(&batch.schema()))?
                    .column(*index)
                    .clone(),
                _ => Err(self.bounds_error(&batch.schema()))?,
            };
        }

        Ok(ColumnarValue::Array(array))
    }

    fn children(&self) -> Vec<Arc<dyn PhysicalExpr>> {
        vec![]
    }

    fn with_new_children(
        self: Arc<Self>,
        _children: Vec<Arc<dyn PhysicalExpr>>,
    ) -> Result<Arc<dyn PhysicalExpr>> {
        Ok(self)
    }

    /// Return the boundaries of this column, if known.
    fn analyze(&self, context: AnalysisContext) -> AnalysisContext {
        assert!(self.index_path[0] < context.column_boundaries.len());
        let col_bounds = context.column_boundaries[self.index_path[0]].clone();
        context.with_boundaries(col_bounds)
    }
}

impl PartialEq<dyn Any> for Column {
    fn eq(&self, other: &dyn Any) -> bool {
        down_cast_any_ref(other)
            .downcast_ref::<Self>()
            .map(|x| self == x)
            .unwrap_or(false)
    }
}

impl Column {
    /// Get the field this expression refers to, given the schema of the input
    fn field(&self, input_schema: &Schema) -> Result<Field> {
        let mut index_iter = self.index_path.iter();
        let mut field = input_schema
            .field(*index_iter.next().ok_or(self.bounds_error(input_schema))?);
        for index in index_iter {
            field = match field.data_type() {
                DataType::Struct(fields) => {
                    fields.get(*index).ok_or(self.bounds_error(input_schema))?
                }
                _ => Err(self.bounds_error(input_schema))?,
            };
        }

        Ok(field.clone())
    }

    fn bounds_error(&self, input_schema: &Schema) -> DataFusionError {
        DataFusionError::Execution(format!(
            "PhysicalExpr Column references column '{}' at index path [{}] but it is out of bounds of the current schema {}",
            self.name,
            self.index_path
                .iter()
                .map(|index| index.to_string())
                .collect::<Vec<String>>()
                .join(", "),
            input_schema
        ))
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct UnKnownColumn {
    name: String,
}

impl UnKnownColumn {
    /// Create a new unknown column expression
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
        }
    }

    /// Get the column name
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Display for UnKnownColumn {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl PhysicalExpr for UnKnownColumn {
    /// Return a reference to Any that can be used for downcasting
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Get the data type of this expression, given the schema of the input
    fn data_type(&self, _input_schema: &Schema) -> Result<DataType> {
        Ok(DataType::Null)
    }

    /// Decide whehter this expression is nullable, given the schema of the input
    fn nullable(&self, _input_schema: &Schema) -> Result<bool> {
        Ok(true)
    }

    /// Evaluate the expression
    fn evaluate(&self, _batch: &RecordBatch) -> Result<ColumnarValue> {
        Err(DataFusionError::Plan(
            "UnKnownColumn::evaluate() should not be called".to_owned(),
        ))
    }

    fn children(&self) -> Vec<Arc<dyn PhysicalExpr>> {
        vec![]
    }

    fn with_new_children(
        self: Arc<Self>,
        _children: Vec<Arc<dyn PhysicalExpr>>,
    ) -> Result<Arc<dyn PhysicalExpr>> {
        Ok(self)
    }
}

impl PartialEq<dyn Any> for UnKnownColumn {
    fn eq(&self, other: &dyn Any) -> bool {
        down_cast_any_ref(other)
            .downcast_ref::<Self>()
            .map(|x| self == x)
            .unwrap_or(false)
    }
}

/// Create a column expression
pub fn col(name: &str, schema: &Schema) -> Result<Arc<dyn PhysicalExpr>> {
    Ok(Arc::new(Column::new_with_schema(name, schema)?))
}

#[cfg(test)]
mod test {
    use crate::expressions::Column;
    use crate::{AnalysisContext, ExprBoundaries, PhysicalExpr};
    use arrow::array::StringArray;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use datafusion_common::{ColumnStatistics, Result, ScalarValue, Statistics};
    use std::sync::Arc;

    #[test]
    fn out_of_bounds_data_type() {
        let schema = Schema::new(vec![Field::new("foo", DataType::Utf8, true)]);
        let col = Column::new("id", 9);
        let error = col.data_type(&schema).expect_err("error");
        assert_eq!("Internal error: PhysicalExpr Column references column 'id' at index 9 (zero-based) \
            but input schema only has 1 columns: [\"foo\"]. This was likely caused by a bug in \
            DataFusion's code and we would welcome that you file an bug report in our issue tracker",
           &format!("{error}"))
    }

    #[test]
    fn out_of_bounds_nullable() {
        let schema = Schema::new(vec![Field::new("foo", DataType::Utf8, true)]);
        let col = Column::new("id", 9);
        let error = col.nullable(&schema).expect_err("error");
        assert_eq!("Internal error: PhysicalExpr Column references column 'id' at index 9 (zero-based) \
            but input schema only has 1 columns: [\"foo\"]. This was likely caused by a bug in \
            DataFusion's code and we would welcome that you file an bug report in our issue tracker",
                   &format!("{error}"))
    }

    #[test]
    fn out_of_bounds_evaluate() -> Result<()> {
        let schema = Schema::new(vec![Field::new("foo", DataType::Utf8, true)]);
        let data: StringArray = vec!["data"].into();
        let batch = RecordBatch::try_new(Arc::new(schema), vec![Arc::new(data)])?;
        let col = Column::new("id", 9);
        let error = col.evaluate(&batch).expect_err("error");
        assert_eq!("Internal error: PhysicalExpr Column references column 'id' at index 9 (zero-based) \
            but input schema only has 1 columns: [\"foo\"]. This was likely caused by a bug in \
            DataFusion's code and we would welcome that you file an bug report in our issue tracker",
                   &format!("{error}"));
        Ok(())
    }

    /// Returns a pair of (schema, statistics) for a table of:
    /// - a => Stats(range=[1, 100], distinct=15)
    /// - b => unknown
    /// - c => Stats(range=[1, 100], distinct=unknown)
    fn get_test_table_stats() -> (Schema, Statistics) {
        let schema = Schema::new(vec![
            Field::new("a", DataType::Int32, true),
            Field::new("b", DataType::Int32, true),
            Field::new("c", DataType::Int32, true),
        ]);

        let columns = vec![
            ColumnStatistics {
                min_value: Some(ScalarValue::Int32(Some(1))),
                max_value: Some(ScalarValue::Int32(Some(100))),
                distinct_count: Some(15),
                ..Default::default()
            },
            ColumnStatistics::default(),
            ColumnStatistics {
                min_value: Some(ScalarValue::Int32(Some(1))),
                max_value: Some(ScalarValue::Int32(Some(75))),
                distinct_count: None,
                ..Default::default()
            },
        ];

        let statistics = Statistics {
            column_statistics: Some(columns),
            ..Default::default()
        };

        (schema, statistics)
    }

    #[test]
    fn stats_bounds_analysis() -> Result<()> {
        let (schema, statistics) = get_test_table_stats();
        let context = AnalysisContext::from_statistics(&schema, &statistics);

        let cases = [
            // (name, index, expected boundaries)
            (
                "a",
                0,
                Some(ExprBoundaries::new(
                    ScalarValue::Int32(Some(1)),
                    ScalarValue::Int32(Some(100)),
                    Some(15),
                )),
            ),
            ("b", 1, None),
            (
                "c",
                2,
                Some(ExprBoundaries::new(
                    ScalarValue::Int32(Some(1)),
                    ScalarValue::Int32(Some(75)),
                    None,
                )),
            ),
        ];

        for (name, index, expected) in cases {
            let col = Column::new(name, index);
            let test_ctx = col.analyze(context.clone());
            assert_eq!(test_ctx.boundaries, expected);
        }

        Ok(())
    }
}
