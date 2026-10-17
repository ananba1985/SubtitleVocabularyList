use std::{
    any::TypeId,
    collections::{BTreeMap, HashSet},
};
use ts_rs::{TS, TypeVisitor};

pub(crate) struct Registry {
    config: ts_rs::Config,
    visited: HashSet<TypeId>,
    types: BTreeMap<String, String>,
    commands: BTreeMap<String, String>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            config: ts_rs::Config::new().with_large_int("number"),
            visited: HashSet::new(),
            types: BTreeMap::new(),
            commands: BTreeMap::new(),
        }
    }
    pub fn include<T: TS + 'static + ?Sized>(&mut self) -> String {
        self.visit::<T>();
        T::name(&self.config)
    }
    pub fn command(&mut self, name: &str, args: String, result: String) {
        assert!(
            self.commands
                .insert(
                    name.into(),
                    format!("  {name}: {{ args: {args}; result: {result} }};\n")
                )
                .is_none(),
            "Duplicate command: {name}"
        );
    }
    pub fn finish(self) -> String {
        let mut text = "// Generated from Rust IPC signatures and DTOs. Run pnpm commands:generate; do not edit.\n\n".to_owned();
        for declaration in self.types.values() {
            text.push_str(&format!("export {declaration}\n\n"));
        }
        text.push_str("export interface CommandMap {\n");
        for declaration in self.commands.values() {
            text.push_str(declaration);
        }
        text.push_str("}\n");
        text
    }
}

impl TypeVisitor for Registry {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        if !self.visited.insert(TypeId::of::<T>()) {
            return;
        }
        T::visit_dependencies(self);
        T::visit_generics(self);
        if T::output_path().is_some() {
            let name = T::ident(&self.config);
            let declaration = T::decl(&self.config);
            if let Some(previous) = self.types.insert(name.clone(), declaration.clone()) {
                assert_eq!(
                    previous, declaration,
                    "Conflicting exported DTO name: {name}"
                );
            }
        }
    }
}
