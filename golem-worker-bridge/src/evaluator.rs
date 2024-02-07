use std::fmt::Display;

use serde_json::Value;

use super::tokeniser::tokeniser::{Token, Tokenizer};
use crate::expr::Expr;
use crate::resolved_variables::{ResolvedVariables, Path};
use crate::typed_json::ValueTyped;

pub trait Evaluator<T> {
    fn evaluate(&self, resolved_variables: &ResolvedVariables) -> Result<T, EvaluationError>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum EvaluationError {
    Message(String),
}

impl Display for EvaluationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvaluationError::Message(string) => write!(f, "{}", string),
        }
    }
}

pub struct Primitive<'t> {
    pub input: &'t str,
}

// When we expect only primitives within a string, and uses ${} not as an expr,
// but as a mere place holder. This type disallows complex structures to end up
// in values such as function-name.
impl<'t> Primitive<'t> {
    pub fn new(str: &'t str) -> Primitive<'t> {
        Primitive { input: str }
    }
}

// Foo/{user-id}
impl<'t> Evaluator<String> for Primitive<'t> {
    fn evaluate(&self, place_holder_values: &ResolvedVariables) -> Result<String, EvaluationError> {
        let mut combined_string = String::new();
        let result: crate::tokeniser::tokeniser::TokeniserResult = Tokenizer::new(self.input).run();

        let mut cursor = result.to_cursor();

        while let Some(token) = cursor.next_token() {
            match token {
                Token::InterpolationStart => {
                    let place_holder_name = cursor
                        .capture_string_between(&Token::InterpolationStart, &Token::CloseParen);

                    if let Some(place_holder_name) = place_holder_name {
                        match place_holder_values.get_key(place_holder_name.as_str()) {
                            Some(place_holder_value) => match place_holder_value {
                                Value::Bool(bool) => {
                                    combined_string.push_str(bool.to_string().as_str())
                                }
                                Value::Number(number) => {
                                    combined_string.push_str(number.to_string().as_str())
                                }
                                Value::String(string) => {
                                    combined_string.push_str(string.to_string().as_str())
                                }

                                _ => {
                                    return Result::Err(EvaluationError::Message(format!(
                                        "Unsupported json type to be replaced in place holder. Make sure the values are primitive {}",
                                        place_holder_name,
                                    )));
                                }
                            },

                            None => {
                                return Result::Err(EvaluationError::Message(format!(
                                    "No value for the place holder {}",
                                    place_holder_name,
                                )));
                            }
                        }
                    }
                }
                token => combined_string.push_str(token.to_string().as_str()),
            }
        }

        Ok(combined_string)
    }
}

impl Evaluator<Value> for Expr {
    // TODO; Bring type variant retruning Result<Variant, EvaluationError>
    fn evaluate(&self, resolved_variables: &ResolvedVariables) -> Result<Value, EvaluationError> {
        let expr: &Expr = self;

        fn go(
            expr: &Expr,
            resolved_variables: &ResolvedVariables,
        ) -> Result<Value, EvaluationError> {
            match expr.clone() {
                Expr::Request() => {
                    match resolved_variables.get_path(&Path::from_string_unsafe(
                        Token::Request.to_string().as_str(),
                    )) {
                        Some(v) => Ok(v),
                        None => Err(EvaluationError::Message(
                            "Details of request is missing".to_string(),
                        )),
                    }
                }
                Expr::WorkerResponse() => {
                    match resolved_variables.get_path(&Path::from_string_unsafe(
                        Token::WorkerResponse.to_string().as_str(),
                    )) {
                        Some(v) => Ok(v),
                        None => Err(EvaluationError::Message(
                            "Details of worker response is missing".to_string(),
                        )),
                    }
                }

                Expr::SelectIndex(expr, index) => {
                    let evaluation_result = go(&expr, resolved_variables)?;

                    evaluation_result.as_array().ok_or(EvaluationError::Message(format!(
                        "Result is not an array to get the index {}",
                        index
                    )))?.get(index).ok_or(EvaluationError::Message(format!(
                        "The array doesn't contain {} elements",
                        index
                    )))
                }

                Expr::SelectField(expr, field_name) => {
                    let evaluation_result = go(&expr, resolved_variables)?;

                    evaluation_result.as_object().ok_or(EvaluationError::Message(format!(
                        "Result is not an object to get the field {}",
                        field_name
                    )))?.get(&field_name).ok_or(EvaluationError::Message(format!(
                        "The result doesn't contain the field {}",
                        field_name
                    )))
                }

                Expr::EqualTo(left, right) => {
                    let left = go(&left, resolved_variables)?;
                    let right = go(&right, resolved_variables)?;

                    let result = ValueTyped::from_json(&left)
                        .equal_to(ValueTyped::from_json(&right))
                        .map_err(|err| EvaluationError::Message(err.to_string()))?;

                    Ok(Value::Bool(result))
                }
                Expr::GreaterThan(left, right) => {
                    let left = go(&left, resolved_variables)?;
                    let right = go(&right, resolved_variables)?;

                    let result = ValueTyped::from_json(&left)
                        .greater_than(ValueTyped::from_json(&right))
                        .map_err(|err| EvaluationError::Message(err.to_string()))?;

                    Ok(Value::Bool(result))
                }
                Expr::GreaterThanOrEqualTo(left, right) => {
                    let left = go(&left, resolved_variables)?;
                    let right = go(&right, resolved_variables)?;

                    let result = ValueTyped::from_json(&left)
                        .greater_than_or_equal_to(ValueTyped::from_json(&right))
                        .map_err(|err| EvaluationError::Message(err.to_string()))?;

                    Ok(Value::Bool(result))
                }
                Expr::LessThan(left, right) => {
                    let left = go(&left, resolved_variables)?;
                    let right = go(&right, resolved_variables)?;
                    let result = ValueTyped::from_json(&left)
                        .less_than(ValueTyped::from_json(&right))
                        .map_err(|err| EvaluationError::Message(err.to_string()))?;

                    Ok(Value::Bool(result))
                }
                Expr::LessThanOrEqualTo(left, right) => {
                    let left = go(&left, resolved_variables)?;
                    let right = go(&right, resolved_variables)?;
                    let result = ValueTyped::from_json(&left)
                        .less_than_or_equal_to(ValueTyped::from_json(&right))
                        .map_err(|err| EvaluationError::Message(err.to_string()))?;

                    Ok(Value::Bool(result))
                }
                Expr::Not(expr) => {
                    let evaluated_expr = expr.evaluate(resolved_variables)?;

                    let bool = evaluated_expr.as_bool().ok_or(EvaluationError::Message(format!(
                        "The expression is evaluated to {} but it is not a boolean expression to apply not (!) operator on",
                        evaluated_expr
                    )))?;

                    Ok(Value::Bool(!bool))
                }

                Expr::Cond(pred0, left, right) => {
                    let pred = go(&pred0, resolved_variables)?;
                    let left = go(&left, resolved_variables)?;
                    let right = go(&right, resolved_variables)?;

                    let bool: bool = pred.as_bool().ok_or(EvaluationError::Message(format!(
                        "The predicate expression is evaluated to {}, but it is not a boolean expression",
                        pred
                    )))?;

                    if bool {
                        Ok(left)
                    } else {
                        Ok(right)
                    }
                }

                Expr::Sequence(exprs) => {
                    let mut result: Vec<Value> = vec![];

                    for expr in exprs {
                        match go(&expr, resolved_variables) {
                            Ok(value) => result.push(value),
                            Err(result) => return Err(result),
                        }
                    }

                    Ok(Value::Array(result))
                }

                Expr::Record(tuples) => {
                    let mut map: serde_json::Map<String, serde_json::Value> =
                        serde_json::Map::new();

                    for (key, expr) in tuples {
                        match go(&expr, resolved_variables) {
                            Ok(variant) => {
                                map.insert(key, variant.convert_to_json());
                            }

                            Err(result) => return Err(result),
                        }
                    }

                    Ok(ValueTyped::ComplexJson(Value::Object(map)))
                }

                Expr::Concat(exprs) => {
                    let mut result = String::new();

                    for expr in exprs {
                        match go(&expr, resolved_variables) {
                            Ok(variant) => {
                                if let Some(primitve) = variant.get_primitive_string() {
                                    result.push_str(primitve.as_str())
                                } else {
                                    return Err(EvaluationError::Message(format!("Cannot append a complex expression {} to form strings. Please check the expression", variant)));
                                }
                            }

                            Err(result) => return Err(result),
                        }
                    }

                    Ok(ValueTyped::String(result))
                }

                Expr::Literal(literal) => Ok(ValueTyped::get_primitive_variant(literal.as_str())),

                Expr::PathVar(path_var) => match resolved_variables.get_key(path_var.as_str()) {
                    Some(value) => match value {
                        Value::Number(number) => {
                            Ok(Value::Number(number))
                        }
                        Value::String(string) => Ok(ValueTyped::from_string(string.as_str())),
                        Value::Bool(bool) => Ok(ValueTyped::from_string(bool.to_string().as_str())),
                        value => Ok(ValueTyped::ComplexJson(value.clone())),
                    },

                    None => Err(EvaluationError::Message(format!(
                        "No value for the place holder {}",
                        path_var,
                    ))),
                },
            }
        }

        go(expr, resolved_variables)
    }
}

#[cfg(test)]
mod tests {
    use crate::evaluator::{EvaluationError, Evaluator};
    use crate::expr::Expr;
    use crate::resolved_variables::{ResolvedVariables, Path};
    use crate::tokeniser::tokeniser::Token;
    use crate::typed_json::ValueTyped;

    fn test_expr(
        expr: Expr,
        expected: Result<ValueTyped, EvaluationError>,
        resolved_variables: &ResolvedVariables,
    ) {
        let result = expr.evaluate(resolved_variables);
        // dbg!("Expr: {:?}", expr);
        // dbg!("Result: {:?}", &result);
        // dbg!("Expected: {:?}", &expected);
        // dbg!("GatewayVariables: {:?}", resolved_variables);
        assert_eq!(result, expected);
    }

    fn test_expr_ok(expr: Expr, expected: ValueTyped, resolved_variables: &ResolvedVariables) {
        test_expr(expr, Ok(expected), resolved_variables);
    }

    fn test_expr_err(expr: Expr, expected: EvaluationError, resolved_variables: &ResolvedVariables) {
        test_expr(expr, Err(expected), resolved_variables);
    }

    fn test_expr_str_ok(expr: &str, expected: &str, resolved_variables: &ResolvedVariables) {
        test_expr_ok(
            Expr::from_primitive_string(expr).expect("Failed to parse expr"),
            ValueTyped::from_string(expected),
            resolved_variables,
        );
    }

    fn test_expr_str_err(expr: &str, expected: &str, resolved_variables: &ResolvedVariables) {
        test_expr_err(
            Expr::from_primitive_string(expr).expect("Failed to parse expr"),
            EvaluationError::Message(expected.to_string()),
            resolved_variables,
        );
    }

    fn get_request_variables(json_str: &str) -> ResolvedVariables {
        let mut resolved_variables = ResolvedVariables::new();

        let v = serde_json::from_str(json_str).expect("Failed to parse json");

        resolved_variables.insert(
            Path::from_string_unsafe(Token::Request.to_string().as_str()),
            v,
        );

        resolved_variables
    }

    #[test]
    fn test_evaluator() {
        let resolved_variables = get_request_variables(
            r#"
                    {
                        "path": {
                           "id": "pId"
                        },
                        "body": {
                           "id": "bId",
                           "name": "bName",
                           "titles": [
                             "bTitle1", "bTitle2"
                           ],
                           "address": {
                             "street": "bStreet",
                             "city": "bCity"
                           }
                        },
                        "headers": {
                           "authorisation": "admin",
                           "content-type": "application/json"
                        }
                    }"#,
        );

        test_expr_str_ok("${request.path.id}", "pId", &resolved_variables);
        test_expr_str_ok("${request.body.id}", "bId", &resolved_variables);
        test_expr_str_ok("${request.body.titles[0]}", "bTitle1", &resolved_variables);
        test_expr_str_ok(
            "${request.body.address.street} ${request.body.address.city}",
            "bStreet bCity",
            &resolved_variables,
        );
        test_expr_str_ok(
            "${if (request.headers.authorisation == \"admin\") then 200 else 401}",
            "401",
            &resolved_variables,
        );
        test_expr_str_err(
            "${request.body.address.street2}",
            "The result doesn't contain the field street2",
            &resolved_variables,
        );
        test_expr_str_err(
            "${request.body.titles[4]}",
            "The array doesn't contain 4 elements",
            &resolved_variables,
        );
        test_expr_str_err(
            "${request.body.address[4]}",
            "Result is not an array to get the index 4",
            &resolved_variables,
        );
        test_expr_str_err(
            "${request.path.id2}",
            "The result doesn't contain the field id2",
            &resolved_variables,
        );
        test_expr_str_err(
            "${if (request.headers.authorisation) then 200 else 401}",
            "The predicate expression is evaluated to admin, but it is not a boolean expression",
            &resolved_variables,
        );
        test_expr_str_err(
            "${request.body.address.street.name}",
            "Cannot obtain field name from a non json value",
            &resolved_variables,
        );
        test_expr_str_err(
            "${worker.response.address.street}",
            "Details of worker response is missing",
            &resolved_variables,
        );
    }
}
