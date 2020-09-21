use super::node::{ExternTensorGraphCondition, NodeEntry};
use crate::ast;
use crate::error::{GraphCallError, Result};
use crate::externs::ExternIR;
use crate::graph::Graph;
use crate::variable::{assert_equal, BuildValue, Link};

#[allow(non_upper_case_globals)]
pub mod builtins {
    pub const INPUTS: &[&str] = &[INPUT_NAME];
    pub(super) const INPUT_NAME: &str = "AssertShape";

    pub(super) const NODE__Transform: &str = "Transform";
    pub(super) const NODE__ToLinear: &str = "ToLinear";
    pub(super) const NODE__Concat: &str = "Concat";
}
use builtins::*;

pub struct GraphNodeEntry<'a, 'b, 'c>
where
    'a: 'b,
    'b: 'c,
{
    pub root: &'c mut NodeEntry<'a, 'b>,
    pub node: ast::GraphNode,
}

// ----------------------
//  BEGIN Default nodes
// ----------------------

struct InputNode;
impl<'a, 'b, 'c> GraphNodeBuilder<InputNode> for GraphNodeEntry<'a, 'b, 'c> {
    fn build(self) -> Result<()> {
        let node = self.node;

        ExternTensorGraphCondition {
            nodes: &[&node].iter().map(|&x| (x.id, x.clone())).collect(),
            names: &["Input"],
            ty_inputs: Some(ast::GraphInputsType::UseLast),
            args: Some(&[]),
            is_sized: Some(true),
            repeatable: Some(false),
            is_id_zero: true,
        }
        .test()?;

        let ir = ExternIR::new(
            INPUT_NAME.to_string(),
            make_empty_graph(&self.root).into(),
            None,
            node.shapes,
        );
        self.root.tensor_graph.push(ir.into());
        Ok(())
    }
}

struct DefaultNode;
impl<'a, 'b, 'c> GraphNodeBuilder<DefaultNode> for GraphNodeEntry<'a, 'b, 'c> {
    fn build(self) -> Result<()> {
        let root = self.root;
        let id = self.node.id;

        for call in self.node.calls {
            // Step 1. get the node
            let mut callee = root.get(&call.name)?;
            let graph = root.graph.borrow();

            callee.set_id(id);
            callee.set_repeat(graph.replace_to(call.repeat)?);

            // Step 2. apply variables
            if let Some(args) = call.args {
                let args = args
                    .into_iter()
                    .map(|(k, v)| {
                        let v = graph.replace_to(Some(v))?;
                        let var = ast::Variable::with_name_value(k.clone(), v);
                        Ok((k, var.into()))
                    })
                    .collect::<Result<_>>()?;
                callee.apply_variables(args, true)?;
            }

            // Step 3. apply IO
            let expected_inputs = callee.get_inputs();
            let given_inputs = unwrap_dict(call.inputs.unwrap_or_default())?;
            *callee.get_inputs_mut() = expected_inputs
                .keys()
                .map(|k| match given_inputs.get(k) {
                    Some(x) => x.clone(),
                    None => ast::Out::with_name(k.clone()),
                })
                .map(|x| (x.name.clone(), x))
                .collect();

            let expected_outputs = callee.get_inputs();
            *callee.get_outputs_mut() = expected_outputs
                .keys()
                .map(|k| ast::Out::new(id, k.clone()))
                .map(|x| (x.name.clone(), x))
                .collect();

            // Step 4. merge shapes
            if root.tensor_graph.is_some() {
                let last_outputs = ast::Shapes::new(
                    callee
                        .get_inputs_mut()
                        .iter_mut()
                        .map(|(k, x)| Ok((k.clone(), root.fetch_shape(x)?)))
                        .collect::<Result<_>>()?,
                );
                let new_inputs = callee.get_input_shapes();

                if let Some(new_inputs) = new_inputs {
                    last_outputs.link_to(new_inputs)?;

                    // identity
                    if let Some(new_outputs) = callee.get_output_shapes() {
                        let mut new_outputs_ref = new_outputs.0.borrow_mut();
                        for (name, out) in new_outputs_ref.iter_mut() {
                            if out.is_none() {
                                let new_outputs_ref = new_inputs.0.borrow();
                                *out = new_outputs_ref[name].clone();
                            }
                        }
                    }
                } else {
                    for x in callee.get_inputs_mut().values_mut() {
                        x.id = Some(0);
                    }
                }
            }

            // Step 5. store
            root.tensor_graph.push(callee.into());
        }

        // Step 6. merge dedicated shapes
        if let Some(shapes) = self.node.shapes {
            if let Some(last_outputs) = root.get_output_shapes() {
                shapes.link_to(last_outputs)?;
            }
        }
        Ok(())
    }
}

// ----------------------
//  BEGIN Built-in nodes
// ----------------------

fn build_transform(
    entry: GraphNodeEntry,
    names: &'static [&'static str; 1],
    linear: bool,
) -> Result<()> {
    let root = entry.root;
    let node = entry.node;

    ExternTensorGraphCondition {
        nodes: &[&node].iter().map(|&x| (x.id, x.clone())).collect(),
        names,
        ty_inputs: Some(ast::GraphInputsType::UseLast),
        args: Some(&[]),
        is_sized: Some(!linear),
        repeatable: Some(false),
        is_id_zero: false,
    }
    .test()?;

    // Step 1. get the IO
    let inputs = root
        .get_output_shapes()
        .ok_or_else(|| GraphCallError::GenericShapes)?;
    let outputs = if linear {
        ast::Shapes::new(
            inputs
                .0
                .borrow()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_ref().map(|x| ast::Shape(vec![x.product()]))))
                .collect(),
        )
    } else {
        node.shapes.unwrap()
    };

    if !linear {
        // Step 2. match the tuple
        let inputs = inputs.0.borrow();
        let outputs = outputs.0.borrow();

        if inputs.len() != outputs.len() || inputs.keys().any(|x| !outputs.contains_key(x)) {
            return GraphCallError::MismatchedShapeKeys {
                expected: inputs.keys().cloned().collect(),
                given: outputs.keys().cloned().collect(),
            }
            .into();
        }

        // Step 3. match the size
        for ((name, input), output) in inputs.iter().zip(outputs.values()) {
            let input = unwrap_value(name, input.as_ref())?.product().build();
            let output = unwrap_value(name, output.as_ref())?.product().build();
            assert_equal(input, output)?;
        }
    }

    // Step 4. store variables
    let graph = make_graph_with_one_var(
        &root,
        "output shapes",
        Some(ast::Value::Map(
            outputs
                .0
                .borrow()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_ref().map(|x| x.0.clone().into())))
                .collect(),
        )),
    );

    // Step 5. store
    let ir = ExternIR::new(
        NODE__Transform.to_string(),
        graph.into(),
        Some(inputs.clone()),
        Some(outputs),
    );
    root.tensor_graph.push(ir.into());
    Ok(())
}

struct Transform;
impl<'a, 'b, 'c> GraphNodeBuilder<Transform> for GraphNodeEntry<'a, 'b, 'c> {
    fn build(self) -> Result<()> {
        build_transform(self, &[NODE__Transform], false)
    }
}

struct ToLinear;
impl<'a, 'b, 'c> GraphNodeBuilder<ToLinear> for GraphNodeEntry<'a, 'b, 'c> {
    fn build(self) -> Result<()> {
        build_transform(self, &[NODE__ToLinear], true)
    }
}

struct Concat;
impl<'a, 'b, 'c> GraphNodeBuilder<Concat> for GraphNodeEntry<'a, 'b, 'c> {
    fn build(self) -> Result<()> {
        let root = self.root;
        let mut node = self.node;

        ExternTensorGraphCondition {
            nodes: &[&node].iter().map(|&x| (x.id, x.clone())).collect(),
            names: &[NODE__Concat],
            ty_inputs: Some(ast::GraphInputsType::List),
            args: Some(&["axis"]),
            is_sized: Some(false),
            repeatable: Some(false),
            is_id_zero: false,
        }
        .test()?;

        let call = node.calls.pop().unwrap();
        let mut args = call.args.unwrap();

        // Step 1. get the axis
        let axis = args.remove("axis");
        let axis = root.graph.borrow().replace_to(axis)?.unwrap();
        let axis = axis.build();

        let mut axis = axis
            .unwrap_int()
            .ok_or_else(|| GraphCallError::MismatchedArgType {
                expected: ast::LetType::UInt,
                given: axis.ty(),
            })?;

        // Step 2. get the inputs
        let mut inputs = call.inputs.unwrap().unwrap_list().unwrap();
        let inputs: Vec<_> = inputs
            .iter_mut()
            .map(|x| root.fetch_shape(x))
            .collect::<Result<_>>()?;

        if inputs.is_empty() {
            return GraphCallError::EmptyInputs.into();
        }

        // Step 3. concat the inputs
        let mut tensor_base: Vec<_> = match &inputs[0] {
            Some(shapes) => shapes.0.iter().map(|x| Some(x)).collect(),
            None => return GraphCallError::GenericShapes.into(),
        };
        let tensor_dims = tensor_base.len() as i64;

        if axis < 0 {
            axis = -axis - tensor_dims;
        }
        if axis < 0 || axis >= tensor_dims {
            return GraphCallError::MismatchedAxis {
                val_min: 0,
                val_max: tensor_dims - 1,
                given: axis,
            }
            .into();
        }

        let axis = axis as usize;
        let tensor_dims = tensor_base.len();

        let mut target_dim = vec![tensor_base[axis].unwrap().clone()];
        tensor_base[axis] = None;

        for (index, shape) in inputs.iter().enumerate().skip(1) {
            let shape = match shape {
                Some(x) => &x.0,
                None => return GraphCallError::GenericListInputShape { index }.into(),
            };

            // test tensor dimensions
            {
                let expected = tensor_dims;
                let given = shape.len();
                if expected != given {
                    return GraphCallError::MismatchedShapes { expected, given }.into();
                }
            }

            // test each tensor dimension
            for (d0, d1) in tensor_base.iter().zip(shape.iter()) {
                if let Some(d0) = d0 {
                    let d0 = d0.build();
                    let d1 = d0.build();
                    assert_equal(d0, d1)?;
                } else {
                    target_dim.push(d1.clone());
                }
            }
        }

        let dim = ast::Shape(target_dim).sum();

        tensor_base[axis] = Some(&dim);
        let outputs: Vec<_> = tensor_base
            .into_iter()
            .map(|x| x.unwrap().clone())
            .collect();

        // Step 4. store variables
        let graph = make_graph_with_one_var(&root, "axis", Some((axis as i64).into()));

        // Step 5. store
        let inputs = inputs
            .iter()
            .enumerate()
            .map(|(i, x)| (format!("{}", i), x.as_ref().cloned()))
            .collect();
        let inputs = ast::Shapes::new(inputs);

        let outputs = ast::Shape(outputs);
        let outputs = [("x", outputs)]
            .iter()
            .map(|(k, v)| (k.to_string(), Some(v.clone())))
            .collect();
        let outputs = ast::Shapes::new(outputs);

        let ir = ExternIR::new(call.name, graph.into(), Some(inputs), Some(outputs));
        root.tensor_graph.push(ir.into());
        Ok(())
    }
}

// ----------------------
//  MATCH Built-in nodes
// ----------------------

macro_rules! match_builtins(
    ($s:ident => $( $t:ty ),*,) => {
        match $s.node.calls[0].name.as_str() {
            $( stringify!($t) => GraphNodeBuilder::<$t>::build($s) ),*,
            _ => GraphNodeBuilder::<DefaultNode>::build($s),
        }
    }
);

impl<'a, 'b, 'c> GraphNodeEntry<'a, 'b, 'c> {
    fn is_input(&self) -> bool {
        self.node.id == 0
    }

    pub fn build(self) -> Result<()> {
        if self.is_input() {
            // input node
            GraphNodeBuilder::<InputNode>::build(self)
        } else {
            match_builtins!(self =>
                Transform,
                ToLinear,
                Concat,
            )
        }
    }
}

// ----------------------
//   END  Built-in nodes
// ----------------------

trait GraphNodeBuilder<B>
where
    Self: Sized,
{
    fn build(self) -> Result<()>;
}

fn make_empty_graph(root: &NodeEntry) -> Graph {
    Graph::new(&root.ctx.root.seed)
}

fn make_graph_with_one_var(root: &NodeEntry, name: &str, value: Option<ast::Value>) -> Graph {
    Graph::with_one_var(&root.ctx.root.seed, name, value)
}

fn unwrap_dict(inputs: ast::GraphInputs) -> Result<ast::Outs> {
    let given = inputs.ty();
    inputs.unwrap_dict().ok_or_else(|| {
        GraphCallError::MismatchedInputsType {
            expected: ast::GraphInputsType::Dict,
            given,
        }
        .into()
    })
}

fn unwrap_value<T>(name: &str, value: Option<T>) -> Result<T> {
    value.ok_or_else(|| {
        GraphCallError::GenericShape {
            name: name.to_string(),
        }
        .into()
    })
}
