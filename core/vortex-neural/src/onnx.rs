//! Pure-Rust ONNX-runtime voor kleine netten: Gemm, MatMul, Add, Sub, Mul, Div, Relu, Sigmoid,
//! Tanh, Softmax, Identity, Flatten (batch × features, f32). Genoeg voor de MLP-layers; grotere
//! modellen (reranker) draaien via llama.cpp. Metadata (layer, kenmerken, klassen) uit
//! `metadata_props`.

use std::collections::{BTreeMap, HashMap};

use prost::Message;

#[allow(clippy::all, clippy::pedantic, missing_debug_implementations)]
pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/onnx.rs"));
}

#[derive(Debug, Clone, PartialEq)]
struct Tensor {
    rows: usize,
    cols: usize,
    data: Vec<f32>,
}

impl Tensor {
    fn at(&self, r: usize, c: usize) -> f32 {
        self.data[r * self.cols + c]
    }
}

#[derive(Debug, Clone)]
enum Op {
    Gemm { alpha: f32, beta: f32, ta: bool, tb: bool },
    MatMul,
    Bin(char),
    Relu,
    Sigmoid,
    Tanh,
    Softmax,
    Identity,
}

#[derive(Debug, Clone)]
struct Node {
    op: Op,
    inputs: Vec<String>,
    output: String,
}

#[derive(Debug, Clone)]
pub struct Net {
    nodes: Vec<Node>,
    init: HashMap<String, Tensor>,
    input: String,
    output: String,
    pub meta: BTreeMap<String, String>,
    pub input_dim: usize,
}

fn tensor(t: &pb::TensorProto) -> Result<Tensor, String> {
    if t.data_type != 1 {
        return Err(format!("tensor {}: alleen float32 (type 1), kreeg {}", t.name, t.data_type));
    }
    let data: Vec<f32> = if !t.raw_data.is_empty() {
        t.raw_data.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
    } else {
        t.float_data.clone()
    };
    let dims: Vec<usize> = t.dims.iter().map(|d| *d as usize).collect();
    let (rows, cols) = match dims.as_slice() {
        [] => (1, 1),
        [n] => (1, *n),
        [r, c] => (*r, *c),
        _ => return Err(format!("tensor {}: alleen 1D/2D", t.name)),
    };
    if rows * cols != data.len() {
        return Err(format!("tensor {}: {}×{} ≠ {} waarden", t.name, rows, cols, data.len()));
    }
    Ok(Tensor { rows, cols, data })
}

impl Net {
    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        let m = pb::ModelProto::decode(b).map_err(|e| format!("geen geldig ONNX-model: {e}"))?;
        let g = m.graph.ok_or("model zonder graaf")?;
        let mut init = HashMap::new();
        for t in &g.initializer {
            init.insert(t.name.clone(), tensor(t)?);
        }
        let mut nodes = Vec::new();
        for n in &g.node {
            let attr_f = |k: &str, d: f32| n.attribute.iter().find(|a| a.name == k).map_or(d, |a| a.f);
            let attr_i = |k: &str, d: i64| n.attribute.iter().find(|a| a.name == k).map_or(d, |a| a.i);
            let op = match n.op_type.as_str() {
                "Gemm" => Op::Gemm {
                    alpha: attr_f("alpha", 1.0),
                    beta: attr_f("beta", 1.0),
                    ta: attr_i("transA", 0) == 1,
                    tb: attr_i("transB", 0) == 1,
                },
                "MatMul" => Op::MatMul,
                "Add" => Op::Bin('+'),
                "Sub" => Op::Bin('-'),
                "Mul" => Op::Bin('*'),
                "Div" => Op::Bin('/'),
                "Relu" => Op::Relu,
                "Sigmoid" => Op::Sigmoid,
                "Tanh" => Op::Tanh,
                "Softmax" => Op::Softmax,
                "Identity" | "Flatten" => Op::Identity,
                other => return Err(format!("operator {other} wordt niet ondersteund (alleen MLP-operatoren)")),
            };
            let output = n.output.first().cloned().ok_or("node zonder uitvoer")?;
            nodes.push(Node { op, inputs: n.input.clone(), output });
        }
        let init_names: std::collections::HashSet<&String> = init.keys().collect();
        let input = g.input.iter().map(|i| i.name.clone()).find(|n| !init_names.contains(n)).ok_or("geen invoer")?;
        let output = g.output.first().map(|o| o.name.clone()).ok_or("geen uitvoer")?;
        let meta: BTreeMap<String, String> =
            m.metadata_props.iter().map(|p| (p.key.clone(), p.value.clone())).collect();
        // Invoerdimensie: uit metadata, anders uit het eerste gewicht dat de invoer raakt.
        let input_dim = meta.get("input_dim").and_then(|v| v.parse().ok()).unwrap_or_else(|| {
            nodes
                .iter()
                .find(|n| n.inputs.first() == Some(&input) || n.inputs.contains(&input))
                .and_then(|n| n.inputs.iter().find_map(|i| init.get(i)))
                .map_or(0, |w| w.cols.max(w.rows))
        });
        Ok(Self { nodes, init, input, output, meta, input_dim })
    }

    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        Self::from_bytes(&std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
    }

    pub fn run(&self, x: &[f32]) -> Result<Vec<f32>, String> {
        Ok(self.run_batch(&[x.to_vec()])?.remove(0))
    }

    pub fn run_batch(&self, xs: &[Vec<f32>]) -> Result<Vec<Vec<f32>>, String> {
        let cols = xs.first().map_or(0, |x| x.len());
        if self.input_dim != 0 && cols != self.input_dim {
            return Err(format!("invoer heeft {cols} kenmerken, het net verwacht {}", self.input_dim));
        }
        let mut vals: HashMap<&str, Tensor> = HashMap::new();
        vals.insert(&self.input, Tensor { rows: xs.len(), cols, data: xs.concat() });
        for n in &self.nodes {
            let get = |k: usize| -> Result<&Tensor, String> {
                let name = n.inputs.get(k).ok_or("ontbrekende invoer")?;
                vals.get(name.as_str()).or_else(|| self.init.get(name)).ok_or_else(|| format!("waarde {name} onbekend"))
            };
            let out = match &n.op {
                Op::Gemm { alpha, beta, ta, tb } => {
                    let (a, b) = (get(0)?, get(1)?);
                    let c = if n.inputs.len() > 2 { Some(get(2)?) } else { None };
                    let (m, k) = if *ta { (a.cols, a.rows) } else { (a.rows, a.cols) };
                    let (k2, nn) = if *tb { (b.cols, b.rows) } else { (b.rows, b.cols) };
                    if k != k2 {
                        return Err(format!("Gemm: {k} ≠ {k2}"));
                    }
                    let mut d = vec![0f32; m * nn];
                    for i in 0..m {
                        for j in 0..nn {
                            let mut s = 0f32;
                            for t in 0..k {
                                let av = if *ta { a.at(t, i) } else { a.at(i, t) };
                                let bv = if *tb { b.at(j, t) } else { b.at(t, j) };
                                s += av * bv;
                            }
                            let cv = c.map_or(0.0, |c| if c.rows == 1 { c.data[j % c.cols] } else { c.at(i, j) });
                            d[i * nn + j] = alpha * s + beta * cv;
                        }
                    }
                    Tensor { rows: m, cols: nn, data: d }
                }
                Op::MatMul => {
                    let (a, b) = (get(0)?, get(1)?);
                    if a.cols != b.rows {
                        return Err(format!("MatMul: {} ≠ {}", a.cols, b.rows));
                    }
                    let mut d = vec![0f32; a.rows * b.cols];
                    for i in 0..a.rows {
                        for t in 0..a.cols {
                            let av = a.at(i, t);
                            for j in 0..b.cols {
                                d[i * b.cols + j] += av * b.at(t, j);
                            }
                        }
                    }
                    Tensor { rows: a.rows, cols: b.cols, data: d }
                }
                Op::Bin(o) => {
                    let (a, b) = (get(0)?, get(1)?);
                    let mut d = a.data.clone();
                    for i in 0..a.rows {
                        for j in 0..a.cols {
                            let bv = if b.data.len() == 1 {
                                b.data[0]
                            } else if b.rows == 1 {
                                b.data[j % b.cols]
                            } else {
                                b.at(i, j)
                            };
                            let x = &mut d[i * a.cols + j];
                            *x = match o {
                                '+' => *x + bv,
                                '-' => *x - bv,
                                '*' => *x * bv,
                                _ => *x / bv,
                            };
                        }
                    }
                    Tensor { rows: a.rows, cols: a.cols, data: d }
                }
                Op::Relu => map(get(0)?, |x| x.max(0.0)),
                Op::Sigmoid => map(get(0)?, |x| 1.0 / (1.0 + (-x).exp())),
                Op::Tanh => map(get(0)?, f32::tanh),
                Op::Identity => get(0)?.clone(),
                Op::Softmax => {
                    let a = get(0)?;
                    let mut d = a.data.clone();
                    for r in d.chunks_mut(a.cols) {
                        let m = r.iter().copied().fold(f32::MIN, f32::max);
                        let s: f32 = r
                            .iter_mut()
                            .map(|x| {
                                *x = (*x - m).exp();
                                *x
                            })
                            .sum();
                        r.iter_mut().for_each(|x| *x /= s);
                    }
                    Tensor { rows: a.rows, cols: a.cols, data: d }
                }
            };
            vals.insert(&n.output, out);
        }
        let y = vals.remove(self.output.as_str()).ok_or("uitvoer niet berekend")?;
        Ok(y.data.chunks(y.cols).map(|c| c.to_vec()).collect())
    }
}

fn map(a: &Tensor, f: impl Fn(f32) -> f32) -> Tensor {
    Tensor { rows: a.rows, cols: a.cols, data: a.data.iter().map(|x| f(*x)).collect() }
}

/// ONNX schrijven vanuit Rust (voor tests en hulpnetten): een MLP met Gemm-lagen (W als
/// [out, in], transB=1 zoals PyTorch) en activaties, optioneel invoernormalisatie.
pub fn mlp_to_onnx(
    layers: &[(Vec<Vec<f32>>, Vec<f32>)],
    hidden_act: &str,
    out_act: &str,
    mean_std: Option<(&[f32], &[f32])>,
    meta: &[(&str, &str)],
) -> Vec<u8> {
    let mut nodes = Vec::new();
    let mut init = Vec::new();
    let t = |name: &str, dims: Vec<i64>, data: Vec<f32>| pb::TensorProto {
        dims,
        data_type: 1,
        float_data: vec![],
        name: name.into(),
        raw_data: data.iter().flat_map(|x| x.to_le_bytes()).collect(),
    };
    let node = |op: &str, inputs: Vec<String>, out: &str, attrs: Vec<pb::AttributeProto>| pb::NodeProto {
        input: inputs,
        output: vec![out.into()],
        name: out.into(),
        op_type: op.into(),
        attribute: attrs,
        domain: String::new(),
    };
    let mut cur = "x".to_string();
    if let Some((m, s)) = mean_std {
        init.push(t("mean", vec![m.len() as i64], m.to_vec()));
        init.push(t("std", vec![s.len() as i64], s.to_vec()));
        nodes.push(node("Sub", vec![cur.clone(), "mean".into()], "xc", vec![]));
        nodes.push(node("Div", vec!["xc".into(), "std".into()], "xn", vec![]));
        cur = "xn".into();
    }
    let in_dim = layers.first().map_or(0, |(w, _)| w.first().map_or(0, |r| r.len()));
    for (i, (w, b)) in layers.iter().enumerate() {
        let (o, inn) = (w.len(), w.first().map_or(0, |r| r.len()));
        init.push(t(&format!("w{i}"), vec![o as i64, inn as i64], w.concat()));
        init.push(t(&format!("b{i}"), vec![o as i64], b.clone()));
        let tb = pb::AttributeProto { name: "transB".into(), i: 1, r#type: 2, ..Default::default() };
        nodes.push(node("Gemm", vec![cur.clone(), format!("w{i}"), format!("b{i}")], &format!("h{i}"), vec![tb]));
        cur = format!("h{i}");
        let act = if i + 1 == layers.len() { out_act } else { hidden_act };
        if !act.is_empty() && act != "identity" {
            let op = match act {
                "relu" => "Relu",
                "tanh" => "Tanh",
                "sigmoid" => "Sigmoid",
                _ => "Softmax",
            };
            nodes.push(node(op, vec![cur.clone()], &format!("a{i}"), vec![]));
            cur = format!("a{i}");
        }
    }
    nodes.push(node("Identity", vec![cur], "y", vec![]));
    let mut props: Vec<pb::StringStringEntryProto> =
        meta.iter().map(|(k, v)| pb::StringStringEntryProto { key: (*k).into(), value: (*v).into() }).collect();
    props.push(pb::StringStringEntryProto { key: "input_dim".into(), value: in_dim.to_string() });
    let m = pb::ModelProto {
        ir_version: 8,
        producer_name: "vortex".into(),
        graph: Some(pb::GraphProto {
            node: nodes,
            name: "mlp".into(),
            initializer: init,
            input: vec![pb::ValueInfoProto { name: "x".into() }],
            output: vec![pb::ValueInfoProto { name: "y".into() }],
            ..Default::default()
        }),
        opset_import: vec![pb::OperatorSetIdProto { domain: String::new(), version: 13 }],
        metadata_props: props,
        ..Default::default()
    };
    m.encode_to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mlp_roundtrip_and_math() {
        // 2 → 2 (relu) → 2 (softmax); met normalisatie.
        let l1 = (vec![vec![1.0, 0.0], vec![0.0, 1.0]], vec![0.0, 0.0]);
        let l2 = (vec![vec![2.0, 0.0], vec![0.0, 2.0]], vec![0.0, 1.0]);
        let bytes = mlp_to_onnx(&[l1, l2], "relu", "softmax", Some((&[1.0, 1.0], &[2.0, 2.0])), &[("layer", "test")]);
        let net = Net::from_bytes(&bytes).unwrap();
        assert_eq!(net.meta["layer"], "test");
        assert_eq!(net.input_dim, 2);
        // x = (5, 1) → norm (2, 0) → relu (2, 0) → (4, 1) → softmax
        let y = net.run(&[5.0, 1.0]).unwrap();
        let e = (4f32 - 1.0).exp();
        assert!((y[0] - e / (e + 1.0)).abs() < 1e-5, "{y:?}");
        assert!((y.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert!(net.run(&[1.0]).is_err(), "verkeerde dimensie");
        let b = net.run_batch(&[vec![5.0, 1.0], vec![1.0, 5.0]]).unwrap();
        assert!(b[0][0] > 0.5 && b[1][1] > 0.5);
    }

    #[test]
    fn rejects_unsupported_ops() {
        let m = pb::ModelProto {
            graph: Some(pb::GraphProto {
                node: vec![pb::NodeProto {
                    input: vec!["x".into()],
                    output: vec!["y".into()],
                    op_type: "Conv".into(),
                    ..Default::default()
                }],
                input: vec![pb::ValueInfoProto { name: "x".into() }],
                output: vec![pb::ValueInfoProto { name: "y".into() }],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(Net::from_bytes(&m.encode_to_vec()).unwrap_err().contains("Conv"));
    }
}
