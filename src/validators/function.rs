use pyo3::exceptions::{PyAssertionError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyAny, PyDict};

use super::{ValError, ValidationError, Validator};
use crate::errors::{val_line_error, ErrorKind, ValResult};
use crate::utils::{dict_get_required, py_error};
use crate::validators::build_validator;

#[derive(Debug, Clone)]
pub struct PreDecoratorValidator {
    validator: Box<dyn Validator>,
    func: PyObject,
}

impl Validator for PreDecoratorValidator {
    fn is_match(type_: &str, dict: &PyDict) -> bool {
        type_ == "decorator" && dict.get_item("pre_decorator").is_some()
    }

    fn build(dict: &PyDict) -> PyResult<Self> {
        Ok(Self {
            validator: build_validator(dict_get_required!(dict, "field", &PyDict)?)?,
            func: get_function(dict, "pre_decorator")?,
        })
    }

    fn validate(&self, py: Python, input: &PyAny, data: &PyDict) -> ValResult<PyObject> {
        let value = self
            .func
            .call(py, (input,), kwargs!(py, "data" => data.as_ref()))
            .map_err(|e| convert_err(py, e, input))?;
        let v: &PyAny = value.as_ref(py);
        self.validator.validate(py, v, data)
    }

    fn clone_dyn(&self) -> Box<dyn Validator> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct PostDecoratorValidator {
    validator: Box<dyn Validator>,
    func: PyObject,
}

impl Validator for PostDecoratorValidator {
    fn is_match(type_: &str, dict: &PyDict) -> bool {
        type_ == "decorator" && dict.get_item("post_decorator").is_some()
    }

    fn build(dict: &PyDict) -> PyResult<Self> {
        Ok(Self {
            validator: build_validator(dict_get_required!(dict, "field", &PyDict)?)?,
            func: get_function(dict, "post_decorator")?,
        })
    }

    fn validate(&self, py: Python, input: &PyAny, data: &PyDict) -> ValResult<PyObject> {
        let v = self.validator.validate(py, input, data)?;
        self.func
            .call(py, (v,), kwargs!(py, "data" => data.as_ref()))
            .map_err(|e| convert_err(py, e, input))
    }

    fn clone_dyn(&self) -> Box<dyn Validator> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct WrapDecoratorValidator {
    validator: Box<dyn Validator>,
    func: PyObject,
}

impl Validator for WrapDecoratorValidator {
    fn is_match(type_: &str, dict: &PyDict) -> bool {
        type_ == "decorator" && dict.get_item("wrap_decorator").is_some()
    }

    fn build(dict: &PyDict) -> PyResult<Self> {
        Ok(Self {
            validator: build_validator(dict_get_required!(dict, "field", &PyDict)?)?,
            func: get_function(dict, "wrap_decorator")?,
        })
    }

    fn validate(&self, py: Python, input: &PyAny, data: &PyDict) -> ValResult<PyObject> {
        let validator_kwarg = ValidatorCallable {
            validator: self.validator.clone(),
            data: data.into_py(py),
        };
        let kwargs = kwargs!(py, "validator" => validator_kwarg, "data" => data.as_ref());
        self.func
            .call(py, (input,), kwargs)
            .map_err(|e| convert_err(py, e, input))
    }

    fn clone_dyn(&self) -> Box<dyn Validator> {
        Box::new(self.clone())
    }
}

#[pyclass]
#[derive(Debug, Clone)]
pub struct ValidatorCallable {
    validator: Box<dyn Validator>,
    data: Py<PyDict>,
}

#[pymethods]
impl ValidatorCallable {
    fn __call__(&self, py: Python, arg: &PyAny) -> PyResult<PyObject> {
        match self.validator.validate(py, arg, self.data.as_ref(py)) {
            Ok(output) => Ok(output),
            Err(ValError::LineErrors(line_errors)) => Err(ValidationError::new_err((line_errors, "Model".to_string()))),
            Err(ValError::InternalErr(err)) => Err(err),
        }
    }

    fn __repr__(&self) -> String {
        format!("ValidatorCallable({:?})", self.validator)
    }
    fn __str__(&self) -> String {
        self.__repr__()
    }
}

fn get_function(dict: &PyDict, key: &str) -> PyResult<PyObject> {
    match dict.get_item(key) {
        Some(obj) => {
            if !obj.is_callable() {
                return py_error!(r#""{}" must be callable"#, key);
            }
            Ok(obj.into_py(obj.py()))
        }
        None => py_error!(r#""{}" is required"#, key),
    }
}

fn convert_err(py: Python, err: PyErr, input: &PyAny) -> ValError {
    let kind = if err.is_instance_of::<PyValueError>(py) {
        ErrorKind::ValueError
    } else if err.is_instance_of::<PyTypeError>(py) {
        ErrorKind::TypeError
    } else if err.is_instance_of::<PyAssertionError>(py) {
        ErrorKind::AssertionError
    } else {
        return ValError::InternalErr(err);
    };

    let message = match err.value(py).str() {
        Ok(s) => Some(s.to_string()),
        Err(err) => return ValError::InternalErr(err),
    };
    #[allow(clippy::redundant_field_names)]
    let line_error = val_line_error!(py, input, kind = kind, message = message);
    ValError::LineErrors(vec![line_error])
}

macro_rules! kwargs {
    ($py:ident, $($k:expr => $v:expr),*) => {{
        Some([$(($k, $v.into_py($py)),)*].into_py_dict($py))
    }};
}
pub(crate) use kwargs;