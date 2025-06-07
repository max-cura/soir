// What we actually emit:
// ```
// type X_Args = ...;
// #[derive(Debug)]
// enum Dispatch {
//   X(X_Args),
//   ...
// }
// fn dispatcher(...) { ... }
// fn x(args: X_Args) -> Dispatch { ... }
// ```

// Step 1: fragmentize
//  - a fragment is a maximal sub-DAG of the value DAG which has all locations the same BUT doesn't
//    have any external inputs which depend on its outputs
// To do this:
//  - construct a value graph from the KNF
//  - take a condensation of the VG

use std::{
    collections::{HashMap, HashSet},
    iter::once,
};

use chumsky::ConfigIterParser;
use la_arena::{Arena, ArenaMap, Idx};
use lasso::{Rodeo, Spur};
use maplit::hashset;
use petgraph::{
    Directed,
    algo::has_path_connecting,
    dot::{self, Dot},
    graph::{DefaultIx, Graph, NodeIndex},
};
use telos_common::{
    source::Sources,
    span::{Span, Spanned},
};
use telos_parser::{lexer::LiteralToken, parser::Pat};

use crate::passes::{
    knf::Ex,
    origin::{Bindings, LocExpr, Placement},
};

pub type Ix = NodeIndex<DefaultIx>;

#[derive(Debug, Clone)]
pub enum Node {
    Match {
        expr: Ix,
        arms: Vec<(Pat, Value)>,
        loc: LocExpr,
    },
    Literal {
        literal: LiteralToken,
    },
    Call {
        func: Value,
        args: Vec<Value>,
        loc: LocExpr,
    },
    Field {
        expr: Ix,
        field: Spur,
        loc: LocExpr,
    },
}
impl Node {
    pub fn location(&self) -> LocExpr {
        match self {
            Node::Literal { literal } => LocExpr::Root,
            Node::Match { loc, .. } | Node::Call { loc, .. } | Node::Field { loc, .. } => {
                loc.clone()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Func(Spur),
    Node(Ix),
    Param(Spur),
}
impl Value {
    fn unwrap_node(self) -> Ix {
        if let Self::Node(x) = self {
            x
        } else {
            panic!("not a node")
        }
    }
}
struct BuildCtx {
    next_anon_func: usize,
    rodeo: Rodeo,
    graph: Graph<Node, (), Directed, DefaultIx>,
    bindings: Bindings<Value>,
    funcs: HashMap<Spur, Value>,
}
impl BuildCtx {
    fn gen_anon_func(&mut self) -> Spur {
        let s = format!("@@{}", self.next_anon_func);
        self.next_anon_func += 1;
        self.rodeo.get_or_intern(&s)
    }
}

fn build_graph(
    expr: Spanned<Idx<Ex>>,
    arena: &Arena<Ex>,
    placements: &ArenaMap<Idx<Ex>, Placement>,
    ctx: &mut BuildCtx,
) -> Value {
    match &arena[*expr] {
        Ex::Match { expr, arms } => {
            //
            todo!()
        }
        Ex::Let { def, body } => {
            let def_node = build_graph(def.expr, arena, placements, ctx);
            ctx.bindings.enter();
            ctx.bindings.insert(def.name, def_node);
            let body_node = build_graph(*body, arena, placements, ctx);
            ctx.bindings.exit();
            body_node
        }
        Ex::LetRec { defs: _, body: _ } => todo!(),
        Ex::Lam { params, body } => {
            ctx.bindings.enter();
            ctx.bindings
                .extend(params.iter().map(|s| (**s, Value::Param(**s))));
            let body_node = build_graph(*body, arena, placements, ctx);
            ctx.bindings.exit();
            let lam_name = ctx.gen_anon_func();
            ctx.funcs.insert(lam_name, body_node);
            Value::Func(lam_name)
        }
        Ex::Literal { literal } => Value::Node(ctx.graph.add_node(Node::Literal {
            literal: literal.clone(),
        })),
        Ex::App { func, args } => {
            let func_expr = ctx.bindings.get(**func).cloned().unwrap();
            let args: Vec<Value> = args
                .iter()
                .map(|arg| ctx.bindings.get(**arg).unwrap())
                .cloned()
                .collect();
            let in_nodes: Vec<Ix> = once(&func_expr)
                .chain(args.iter())
                .filter_map(|v| match v {
                    Value::Node(node) => Some(node),
                    _ => None,
                })
                .copied()
                .collect();
            let new_node = ctx.graph.add_node(Node::Call {
                func: func_expr,
                args,
                loc: placements[*expr].location.clone().unwrap(),
            });
            for in_node in in_nodes {
                ctx.graph.add_edge(in_node, new_node, ());
            }
            Value::Node(new_node)
        }
        Ex::Field {
            expr: field_expr,
            field,
        } => {
            let field_expr_node = ctx
                .bindings
                .get(**field_expr)
                .unwrap()
                .clone()
                .unwrap_node();
            let new_node = ctx.graph.add_node(Node::Field {
                expr: field_expr_node,
                field: **field,
                loc: placements[*expr].location.clone().unwrap(),
            });
            ctx.graph.add_edge(field_expr_node, new_node, ());
            Value::Node(new_node)
        }
        Ex::Var { name } => ctx.bindings.get(*name).unwrap().clone(),
    }
}

pub fn run(
    inputs: &[(Span, Spur, Vec<Spanned<Spur>>, Spanned<Idx<Ex>>)],
    rodeo: Rodeo,
    arena: &mut Arena<Ex>,
    placements: &ArenaMap<Idx<Ex>, Placement>,
    builtins: &HashMap<Spur, Value>,
    sources: &Sources,
) {
    let mut ctx = BuildCtx {
        next_anon_func: 0,
        rodeo,
        graph: Graph::new(),
        bindings: Bindings::default(),
        funcs: HashMap::new(),
    };
    ctx.bindings.enter();
    ctx.bindings.extend(builtins.clone());
    ctx.bindings.enter();

    for (tl_span, tl_name, tl_params, tl_body) in inputs {
        let fake_lam = arena.alloc(Ex::Lam {
            params: tl_params.clone(),
            body: *tl_body,
        });
        let binding = build_graph(
            Spanned::new(fake_lam, *tl_span),
            arena,
            placements,
            &mut ctx,
        );
        ctx.bindings.insert(*tl_name, binding);
    }

    ctx.bindings.exit();
    ctx.bindings.exit();

    let BuildCtx { rodeo, graph, .. } = ctx;

    let s_graph = graph.map(
        |i, n| match n {
            Node::Match { expr, arms, loc: _ } => {
                format!("{}=match {}{{ <todo: display> }}", i.index(), expr.index());
                todo!("match")
            }
            Node::Literal { literal } => {
                let mut s = format!("{}=", i.index());
                let _ = literal.fmt(&mut s, &rodeo);
                s
            }
            Node::Call { func, args, loc: _ } => {
                format!(
                    "{}=call {}({})",
                    i.index(),
                    match func {
                        Value::Func(spur) => format!("{}", rodeo.resolve(spur)),
                        Value::Node(node_index) => format!("{}", node_index.index()),
                        Value::Param(spur) => format!("{}", rodeo.resolve(spur)),
                    },
                    args.iter()
                        .map(|arg| match arg {
                            Value::Func(spur) => format!("{}", rodeo.resolve(spur)),
                            Value::Node(node_index) => format!("{}", node_index.index()),
                            Value::Param(spur) => format!("{}", rodeo.resolve(spur)),
                        })
                        .intersperse(String::from(" "))
                        .collect::<String>()
                )
            }
            Node::Field {
                expr,
                field,
                loc: _,
            } => {
                format!("{}={}.{}", i.index(), expr.index(), rodeo.resolve(field))
            }
        },
        |_, _| (),
    );
    std::fs::write(
        "graph.dot",
        format!(
            "{:?}",
            Dot::with_config(&s_graph, &[dot::Config::EdgeNoLabel])
        ),
    )
    .unwrap();

    let mut patches: Vec<(LocExpr, Vec<Ix>)> = vec![];
    let vg_sorted = petgraph::algo::toposort(&graph, None).unwrap();
    for node in vg_sorted {
        // get location
        let loc = graph[node].location();
        if let Some(pre) = patches.iter().position(|(l, _)| l == &loc) {
            patches[pre].1.push(node);
        } else {
            patches.push((loc, vec![node]))
        }
    }
    // tracing::debug!("patches = {patches:?}");
    let mut vg_wcc_initial = vec![];
    for (loc, nodeset) in patches {
        let mut components: Vec<HashSet<Ix>> = vec![];
        for node in nodeset {
            let mut found = false;
            for comp in &mut components {
                // tracing::debug!("{node:?} has predecessors in {comp:?}");
                let mut ffound = None;
                for comp_node in comp.iter() {
                    if graph.contains_edge(*comp_node, node) {
                        ffound = Some(node);
                        found = true;
                    }
                }
                if let Some(ff) = ffound {
                    comp.insert(ff);
                }
            }
            if !found {
                components.push(hashset![node]);
            }
        }
        // tracing::debug!("loc = {loc:?}, components = {components:?}");
        vg_wcc_initial.extend(
            components
                .into_iter()
                .flat_map(|comps| std::iter::repeat(loc.clone()).zip(comps.into_iter())),
        );
    }
    // okay, now we have an initial list of WCCs (that may be incorrect)
    // vg_wcc_initial
}
