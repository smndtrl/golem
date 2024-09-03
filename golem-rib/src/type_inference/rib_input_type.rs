use crate::Expr;
use bincode::{Decode, Encode};
use golem_api_grpc::proto::golem::rib::RibInputType as ProtoRibInputType;
use golem_wasm_ast::analysis::AnalysedType;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode)]
pub struct RibInputTypeInfo {
    pub types: HashMap<String, AnalysedType>,
}
impl RibInputTypeInfo {
    pub fn empty() -> Self {
        RibInputTypeInfo {
            types: HashMap::new(),
        }
    }

    pub fn from_expr(expr: &mut Expr) -> Result<RibInputTypeInfo, String> {
        let mut queue = VecDeque::new();

        let mut global_variables = HashMap::new();

        queue.push_back(expr);

        while let Some(expr) = queue.pop_back() {
            match expr {
                Expr::Identifier(variable_id, inferred_type) => {
                    if variable_id.is_global() {
                        let analysed_type = AnalysedType::try_from(&*inferred_type)?;
                        global_variables.insert(variable_id.name(), analysed_type);
                    }
                }
                _ => expr.visit_children_mut_bottom_up(&mut queue),
            }
        }

        Ok(RibInputTypeInfo {
            types: global_variables,
        })
    }
}

impl TryFrom<ProtoRibInputType> for RibInputTypeInfo {
    type Error = String;
    fn try_from(value: ProtoRibInputType) -> Result<Self, String> {
        let mut types = HashMap::new();
        for (key, value) in value.types {
            types.insert(key, AnalysedType::try_from(&value)?);
        }
        Ok(RibInputTypeInfo { types })
    }
}

impl From<RibInputTypeInfo> for ProtoRibInputType {
    fn from(value: RibInputTypeInfo) -> Self {
        let mut types = HashMap::new();
        for (key, value) in value.types {
            types.insert(key, golem_wasm_ast::analysis::protobuf::Type::from(&value));
        }
        ProtoRibInputType { types }
    }
}