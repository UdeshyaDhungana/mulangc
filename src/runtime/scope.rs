//! Variable bindings and the chain of enclosing scopes.
//!
//! The distinction that matters here is between *declaring* and *assigning*.
//! `thoos_muji` declares, always in the innermost scope, shadowing anything
//! outside. `=` assigns, and must find an existing binding somewhere along the
//! chain — walking that chain is this module's job, not its callers'.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::runtime::value::Value;

/// Scopes are shared: a closure holds on to the one it was written in, which
/// may still be the current scope of a call in progress.
pub type ScopeHandle = Rc<RefCell<Scope>>;

pub struct Scope {
    bindings: HashMap<String, Value>,
    enclosing: Option<ScopeHandle>,
}

impl Scope {
    pub fn global() -> ScopeHandle {
        Rc::new(RefCell::new(Scope {
            bindings: HashMap::new(),
            enclosing: None,
        }))
    }

    /// A fresh empty scope nested inside `enclosing` — one call's parameters,
    /// one iteration of a loop, one branch of a conditional.
    pub fn nested_in(enclosing: &ScopeHandle) -> ScopeHandle {
        Rc::new(RefCell::new(Scope {
            bindings: HashMap::new(),
            enclosing: Some(Rc::clone(enclosing)),
        }))
    }

    /// Introduces `name` in this scope, replacing any binding of the same name
    /// already here and shadowing any in an enclosing scope.
    pub fn declare(&mut self, name: &str, value: Value) {
        self.bindings.insert(name.to_owned(), value);
    }

    pub fn lookup(&self, name: &str) -> Option<Value> {
        match self.bindings.get(name) {
            Some(value) => Some(value.clone()),
            None => self.enclosing.as_ref()?.borrow().lookup(name),
        }
    }

    /// Rebinds the innermost existing `name`. Returns `false` when there is
    /// none, which is how the interpreter distinguishes assignment from a
    /// typo'd variable.
    pub fn assign(&mut self, name: &str, value: Value) -> bool {
        if let Some(existing) = self.bindings.get_mut(name) {
            *existing = value;
            return true;
        }
        match &self.enclosing {
            Some(enclosing) => enclosing.borrow_mut().assign(name, value),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nested_scope_sees_its_enclosing_bindings() {
        let outer = Scope::global();
        outer.borrow_mut().declare("x", Value::Integer(1));
        let inner = Scope::nested_in(&outer);

        assert!(inner.borrow().lookup("x").is_some());
    }

    #[test]
    fn declaring_shadows_rather_than_overwrites() {
        let outer = Scope::global();
        outer.borrow_mut().declare("x", Value::Integer(1));
        let inner = Scope::nested_in(&outer);
        inner.borrow_mut().declare("x", Value::Integer(2));

        assert!(matches!(
            inner.borrow().lookup("x"),
            Some(Value::Integer(2))
        ));
        assert!(matches!(
            outer.borrow().lookup("x"),
            Some(Value::Integer(1))
        ));
    }

    #[test]
    fn assigning_reaches_out_to_the_enclosing_binding() {
        let outer = Scope::global();
        outer.borrow_mut().declare("x", Value::Integer(1));
        let inner = Scope::nested_in(&outer);

        assert!(inner.borrow_mut().assign("x", Value::Integer(2)));
        assert!(matches!(
            outer.borrow().lookup("x"),
            Some(Value::Integer(2))
        ));
    }

    #[test]
    fn assigning_to_an_undeclared_name_fails() {
        let scope = Scope::global();
        assert!(!scope.borrow_mut().assign("missing", Value::Null));
        assert!(scope.borrow().lookup("missing").is_none());
    }
}
