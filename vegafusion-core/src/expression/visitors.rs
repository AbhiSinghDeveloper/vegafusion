/*
 * VegaFusion
 * Copyright (C) 2022 VegaFusion Technologies LLC
 *
 * This program is distributed under multiple licenses.
 * Please consult the license documentation provided alongside
 * this program the details of the active license.
 */
use crate::proto::gen::expression::property::Key;
use crate::proto::gen::expression::{
    ArrayExpression, BinaryExpression, CallExpression, ConditionalExpression, Expression,
    Identifier, Literal, LogicalExpression, MemberExpression, ObjectExpression, UnaryExpression,
};

use crate::expression::column_usage::{ColumnUsage, VlSelectionFields};
use crate::expression::supported::{
    ALL_DATA_FNS, ALL_EXPRESSION_CONSTANTS, ALL_SCALE_FNS, IMPLICIT_VARS, SUPPORTED_DATA_FNS,
    SUPPORTED_EXPRESSION_FNS, SUPPORTED_SCALE_FNS,
};
use crate::proto::gen::expression::expression::Expr;
use crate::proto::gen::expression::literal::Value;
use crate::proto::gen::tasks::Variable;
use crate::task_graph::task::InputVariable;
use std::collections::{HashMap, HashSet};

pub trait ExpressionVisitor {
    fn visit_expression(&mut self, _expression: &Expression) {}
    fn visit_identifier(&mut self, _node: &Identifier) {}
    fn visit_called_identifier(&mut self, _node: &Identifier, _args: &[Expression]) {}
    fn visit_literal(&mut self, _node: &Literal) {}
    fn visit_binary(&mut self, _node: &BinaryExpression) {}
    fn visit_logical(&mut self, _node: &LogicalExpression) {}
    fn visit_unary(&mut self, _node: &UnaryExpression) {}
    fn visit_conditional(&mut self, _node: &ConditionalExpression) {}
    fn visit_member(&mut self, _node: &MemberExpression) {}
    fn visit_call(&mut self, _node: &CallExpression) {}
    fn visit_array(&mut self, _node: &ArrayExpression) {}
    fn visit_object(&mut self, _node: &ObjectExpression) {}
    fn visit_object_key(&mut self, _node: &Key) {}
    fn visit_static_member_identifier(&mut self, _node: &Identifier) {}
}

pub trait MutExpressionVisitor {
    fn visit_expression(&mut self, _expression: &mut Expression) {}
    fn visit_identifier(&mut self, _node: &mut Identifier) {}
    fn visit_called_identifier(&mut self, _node: &mut Identifier, _args: &mut [Expression]) {}
    fn visit_literal(&mut self, _node: &mut Literal) {}
    fn visit_binary(&mut self, _node: &mut BinaryExpression) {}
    fn visit_logical(&mut self, _node: &mut LogicalExpression) {}
    fn visit_unary(&mut self, _node: &mut UnaryExpression) {}
    fn visit_conditional(&mut self, _node: &mut ConditionalExpression) {}
    fn visit_member(&mut self, _node: &mut MemberExpression) {}
    fn visit_call(&mut self, _node: &mut CallExpression) {}
    fn visit_array(&mut self, _node: &mut ArrayExpression) {}
    fn visit_object(&mut self, _node: &mut ObjectExpression) {}
    fn visit_object_key(&mut self, _node: &mut Key) {}
    fn visit_static_member_identifier(&mut self, _node: &mut Identifier) {}
}

/// Visitor to set all spans in the expression tree to None
#[derive(Clone, Default)]
pub struct ClearSpansVisitor {}
impl ClearSpansVisitor {
    pub fn new() -> Self {
        Self {}
    }
}

impl MutExpressionVisitor for ClearSpansVisitor {
    fn visit_expression(&mut self, expression: &mut Expression) {
        expression.span.take();
    }
    fn visit_member(&mut self, node: &mut MemberExpression) {
        node.property.as_mut().unwrap().span.take();
    }
}

/// Visitor to collect all unbound input variables in the expression
#[derive(Clone, Default)]
pub struct GetInputVariablesVisitor {
    pub input_variables: HashSet<InputVariable>,
    pub expression_fns: HashSet<String>,
    pub data_fns: HashSet<String>,
    pub scale_fns: HashSet<String>,
}

impl GetInputVariablesVisitor {
    pub fn new() -> Self {
        Self {
            input_variables: Default::default(),
            expression_fns: Default::default(),
            data_fns: Default::default(),
            scale_fns: Default::default(),
        }
    }
}

impl ExpressionVisitor for GetInputVariablesVisitor {
    fn visit_identifier(&mut self, node: &Identifier) {
        // implicit vars like datum and event do not count as a variables
        if !IMPLICIT_VARS.contains(node.name.as_str())
            && !ALL_EXPRESSION_CONSTANTS.contains(node.name.as_str())
        {
            self.input_variables.insert(InputVariable {
                var: Variable::new_signal(&node.name),
                propagate: true,
            });
        }
    }

    /// Collect data and scale identifiers. These show up as a literal string as the first
    /// argument to a Data or Scale callable.
    fn visit_called_identifier(&mut self, node: &Identifier, args: &[Expression]) {
        if let Some(arg0) = args.get(0) {
            if let Ok(arg0) = arg0.as_literal() {
                if let Value::String(arg0) = arg0.value() {
                    // Check data callable
                    if ALL_DATA_FNS.contains(node.name.as_str()) {
                        // Propagate on changes to data unless this is a modify function
                        let propagate = node.name != "modify";
                        self.input_variables.insert(InputVariable {
                            var: Variable::new_data(arg0),
                            propagate,
                        });
                    }

                    // Check scale callable
                    if ALL_SCALE_FNS.contains(node.name.as_str()) {
                        self.input_variables.insert(InputVariable {
                            var: Variable::new_scale(arg0),
                            propagate: true,
                        });
                    }
                }
            }
        }

        // Record function type
        if ALL_DATA_FNS.contains(node.name.as_str()) {
            self.data_fns.insert(node.name.clone());
        } else if ALL_SCALE_FNS.contains(node.name.as_str()) {
            self.scale_fns.insert(node.name.clone());
        } else {
            self.expression_fns.insert(node.name.clone());
        }
    }
}

/// Visitor to collect all output variables in the expression
#[derive(Clone, Default)]
pub struct UpdateVariablesExprVisitor {
    pub update_variables: HashSet<Variable>,
}

impl UpdateVariablesExprVisitor {
    pub fn new() -> Self {
        Self {
            update_variables: Default::default(),
        }
    }
}

impl ExpressionVisitor for UpdateVariablesExprVisitor {
    fn visit_called_identifier(&mut self, node: &Identifier, args: &[Expression]) {
        if node.name == "modify" {
            if let Some(arg0) = args.get(0) {
                if let Ok(arg0) = arg0.as_literal() {
                    if let Value::String(arg0) = arg0.value() {
                        // First arg is a string, which holds the name of the output dataset
                        self.update_variables.insert(Variable::new_data(arg0));
                    }
                }
            }
        }
    }
}

/// Visitor to check whether an expression is supported by the VegaFusion Runtime
#[derive(Clone, Default)]
pub struct CheckSupportedExprVisitor {
    pub supported: bool,
}

impl CheckSupportedExprVisitor {
    pub fn new() -> Self {
        Self { supported: true }
    }
}

impl ExpressionVisitor for CheckSupportedExprVisitor {
    fn visit_called_identifier(&mut self, node: &Identifier, args: &[Expression]) {
        // Check for unsupported functions
        if ALL_DATA_FNS.contains(node.name.as_str()) {
            if !SUPPORTED_DATA_FNS.contains(node.name.as_str()) {
                self.supported = false;
            }
            if node.name == "vlSelectionResolve" && args.len() > 2 {
                // The third (multi) and forth (vl5) arguments are not supported
                self.supported = false;
            }
        } else if ALL_SCALE_FNS.contains(node.name.as_str()) {
            if !SUPPORTED_SCALE_FNS.contains(node.name.as_str()) {
                self.supported = false;
            }
        } else if !SUPPORTED_EXPRESSION_FNS.contains(node.name.as_str()) {
            self.supported = false;
        }
    }

    fn visit_member(&mut self, node: &MemberExpression) {
        // Check for unsupported use of member property.
        // Property cannot use implicit datum variable
        if node.computed {
            let property = node.property.as_ref().unwrap();
            if property.implicit_vars().contains(&"datum".to_string()) {
                self.supported = false;
            }
        }
    }
}

/// Visitor to collect all implicit variables used in an expression
#[derive(Clone, Default)]
pub struct ImplicitVariablesExprVisitor {
    pub implicit_vars: HashSet<String>,
}

impl ImplicitVariablesExprVisitor {
    pub fn new() -> Self {
        Self {
            implicit_vars: Default::default(),
        }
    }
}

impl ExpressionVisitor for ImplicitVariablesExprVisitor {
    fn visit_identifier(&mut self, node: &Identifier) {
        // implicit vars like datum and event do not count as a variables
        if IMPLICIT_VARS.contains(node.name.as_str()) {
            self.implicit_vars.insert(node.name.clone());
        }
    }
}

/// Visitor to collect the columns
#[derive(Clone)]
pub struct DatumColumnUsageVisitor<'a> {
    pub vl_selection_fields: &'a VlSelectionFields,
    pub column_usage: ColumnUsage,
}

impl<'a> DatumColumnUsageVisitor<'a> {
    pub fn new(vl_selection_fields: &'a HashMap<String, Vec<String>>) -> Self {
        Self {
            vl_selection_fields,
            column_usage: ColumnUsage::Known(Default::default()),
        }
    }
}

impl<'a> ExpressionVisitor for DatumColumnUsageVisitor<'a> {
    fn visit_member(&mut self, node: &MemberExpression) {
        if let (Some(object), Some(property)) = (&node.object, &node.property) {
            if let (Some(Expr::Identifier(object_id)), Some(property_expr)) =
                (&object.expr, &property.expr)
            {
                if object_id.name == "datum" {
                    // This expression is a member expression on the datum free variable
                    if node.computed {
                        match property_expr {
                            Expr::Literal(Literal {
                                value: Some(Value::String(name)),
                                ..
                            }) => {
                                // Found `datum['col_name']` usage
                                self.column_usage = self.column_usage.with_column(&name);
                            }
                            _ => {
                                // Unknown usage
                                self.column_usage = ColumnUsage::Unknown;
                            }
                        }
                    } else {
                        match property_expr {
                            Expr::Identifier(id) => {
                                // Found `datum.col_name` usage
                                self.column_usage = self.column_usage.with_column(&id.name);
                            }
                            _ => {
                                // Unknown usage
                                self.column_usage = ColumnUsage::Unknown;
                            }
                        }
                    }
                }
            }
        }
    }

    fn visit_call(&mut self, node: &CallExpression) {
        if node.callee == "vlSelectionTest" {
            // First argument should be a string
            if let Some(Expression {
                expr:
                    Some(Expr::Literal(Literal {
                        value: Some(Value::String(dataset_name)),
                        ..
                    })),
                ..
            }) = node.arguments.get(0)
            {
                if let Some(fields) = self.vl_selection_fields.get(dataset_name) {
                    // Add selection fields to usage
                    let usage = ColumnUsage::from(fields.as_slice());
                    self.column_usage = self.column_usage.union(&usage);
                } else {
                    // Unknown fields dataset
                    self.column_usage = ColumnUsage::Unknown;
                }
            } else {
                // Unknown usage
                self.column_usage = ColumnUsage::Unknown;
            }
        }
    }
}
