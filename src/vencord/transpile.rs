//! Vencord's plugins are TypeScript with JSX; QuickJS wants JavaScript. Types go, JSX becomes
//! calls to the same globals Vencord's own build uses, and imports and exports become CommonJS,
//! so a Discord module Rustcord doesn't have can be a stub instead of a link error.

use oxc::allocator::Allocator;
use oxc::ast::AstKind;
use oxc::ast::ast::{Declaration, ExportDefaultDeclarationKind, ImportDeclarationSpecifier, ModuleExportName, Statement};
use oxc::codegen::Codegen;
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::{GetSpan, SourceType};
use oxc::transformer::{JsxOptions, JsxRuntime, TransformOptions, Transformer};
use std::path::Path;

pub fn transpile(path: &str, src: &str) -> Result<String, String> {
    let alloc = Allocator::default();
    let ty = SourceType::from_path(path).map_err(|e| e.to_string())?.with_module(true);
    let parsed = Parser::new(&alloc, src, ty).parse();
    if let Some(e) = parsed.diagnostics.first() {
        return Err(format!("{path}: {e}"));
    }
    let mut program = parsed.program;
    let scoping = SemanticBuilder::new().with_enum_eval(true).build(&program).semantic.into_scoping();
    let opts = TransformOptions {
        jsx: JsxOptions {
            runtime: JsxRuntime::Classic,
            pragma: Some("VencordCreateElement".into()),
            pragma_frag: Some("VencordFragment".into()),
            pure: false,
            ..JsxOptions::enable()
        },
        ..TransformOptions::default()
    };
    let done = Transformer::new(&alloc, Path::new(path), &opts).build_with_scoping(scoping, &mut program);
    if let Some(e) = done.diagnostics.first() {
        return Err(format!("{path}: {e}"));
    }
    to_cjs(path, &Codegen::new().build(&program).code)
}

fn lit(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

fn export_name(n: &ModuleExportName) -> String {
    n.name().to_string()
}

/// Rewrites a module's imports and exports in place. Imported names become reads through the
/// module object (`__m0.foo`), so they stay live like ESM bindings; exports become getters
/// declared up front.
fn to_cjs(path: &str, js: &str) -> Result<String, String> {
    let alloc = Allocator::default();
    let parsed = Parser::new(&alloc, js, SourceType::mjs()).parse();
    if let Some(e) = parsed.diagnostics.first() {
        return Err(format!("{path}: {e}"));
    }
    let program = parsed.program;
    let sem = SemanticBuilder::new().with_build_nodes(true).build(&program).semantic;
    let (scoping, nodes) = (sem.scoping(), sem.nodes());
    let mut edits: Vec<(u32, u32, String)> = Vec::new();
    let mut head = String::from("Object.defineProperty(exports, \"__esModule\", { value: true });\n");
    let getter = |head: &mut String, name: &str, expr: &str| {
        head.push_str(&format!("Object.defineProperty(exports, {}, {{ enumerable: true, configurable: true, get: () => {expr} }});\n", lit(name)));
    };
    // local name -> what reads it, for imports exported again by name
    let mut imported: Vec<(String, String)> = Vec::new();
    let mut n = 0;
    let mut module = || {
        n += 1;
        format!("__m{}", n - 1)
    };
    for st in &program.body {
        match st {
            Statement::ImportDeclaration(d) => {
                let m = module();
                edits.push((d.span.start, d.span.end, format!("const {m} = __require({});", lit(d.source.value.as_str().unwrap_or_default()))));
                for sp in d.specifiers.iter().flatten() {
                    let (local, read) = match sp {
                        ImportDeclarationSpecifier::ImportSpecifier(s) => (&s.local, format!("{m}[{}]", lit(&export_name(&s.imported)))),
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => (&s.local, format!("{m}.default")),
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => (&s.local, m.clone()),
                    };
                    imported.push((local.name.to_string(), read.clone()));
                    for r in scoping.get_resolved_reference_ids(local.symbol_id()) {
                        let id = scoping.get_reference(*r).node_id();
                        let span = nodes.get_node(id).kind().span();
                        let text = match nodes.parent_kind(id) {
                            AstKind::ObjectProperty(p) if p.shorthand => format!("{}: {read}", local.name),
                            _ => read.clone(),
                        };
                        edits.push((span.start, span.end, text));
                    }
                }
            }
            Statement::ExportDeclaration(d) => {
                edits.push((d.span.start, d.declaration.span().start, String::new()));
                let names: Vec<String> = match &d.declaration {
                    Declaration::VariableDeclaration(v) => v.declarations.iter().flat_map(|x| x.id.get_binding_identifiers()).map(|b| b.name.to_string()).collect(),
                    Declaration::FunctionDeclaration(f) => f.id.iter().map(|b| b.name.to_string()).collect(),
                    Declaration::ClassDeclaration(c) => c.id.iter().map(|b| b.name.to_string()).collect(),
                    _ => Vec::new(),
                };
                for name in names {
                    getter(&mut head, &name, &name);
                }
            }
            Statement::ExportDefaultDeclaration(d) => {
                let named = match &d.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(f) => f.id.as_ref().map(|b| b.name.to_string()),
                    ExportDefaultDeclarationKind::ClassDeclaration(c) => c.id.as_ref().map(|b| b.name.to_string()),
                    _ => None,
                };
                let start = d.declaration.span().start;
                match named {
                    Some(name) => {
                        edits.push((d.span.start, start, String::new()));
                        getter(&mut head, "default", &name);
                    }
                    None => edits.push((d.span.start, start, "exports.default = ".into())),
                }
            }
            Statement::ExportNamedDeclaration(d) => {
                edits.push((d.span.start, d.span.end, String::new()));
                for s in &d.specifiers {
                    let local = export_name(&s.local);
                    let read = imported.iter().find(|(l, _)| *l == local).map_or(local.clone(), |(_, r)| r.clone());
                    getter(&mut head, &export_name(&s.exported), &read);
                }
            }
            Statement::ExportFromDeclaration(d) => {
                let m = module();
                edits.push((d.span.start, d.span.end, format!("const {m} = __require({});", lit(d.source.value.as_str().unwrap_or_default()))));
                for s in &d.specifiers {
                    getter(&mut head, &export_name(&s.exported), &format!("{m}[{}]", lit(&export_name(&s.local))));
                }
            }
            Statement::ExportAllDeclaration(d) => {
                let req = format!("__require({})", lit(d.source.value.as_str().unwrap_or_default()));
                match &d.exported {
                    Some(ns) => {
                        let m = module();
                        edits.push((d.span.start, d.span.end, format!("const {m} = {req};")));
                        getter(&mut head, &export_name(ns), &m);
                    }
                    None => edits.push((d.span.start, d.span.end, format!("__star(exports, {req});"))),
                }
            }
            _ => {}
        }
    }
    // a reference inside a statement that's replaced whole goes with it
    edits.sort_by_key(|e| (e.0, std::cmp::Reverse(e.1)));
    let mut kept: Vec<(u32, u32, String)> = Vec::new();
    for e in edits {
        if kept.last().is_none_or(|k| e.0 >= k.1) {
            kept.push(e);
        }
    }
    let mut out = js.to_string();
    for (a, b, text) in kept.into_iter().rev() {
        out.replace_range(a as usize..b as usize, &text);
    }
    Ok(head + &out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn strips_types_and_jsx() {
        let js = super::transpile(
            "index.tsx",
            r#"import definePlugin, { OptionType } from "@utils/types";
const enum Mode { A = 1 }
interface P { x: number }
export default definePlugin({ name: "T", render: (p: P) => <div className="a">{p.x}</div>, mode: Mode.A, opt: OptionType.BOOLEAN });"#,
        )
        .unwrap();
        assert!(js.contains("VencordCreateElement(\"div\""), "{js}");
        assert!(!js.contains("interface"), "{js}");
        assert!(js.contains("__require(\"@utils/types\")"), "{js}");
        assert!(js.contains("__m0.default({"), "{js}");
        assert!(js.contains("__m0[\"OptionType\"].BOOLEAN"), "{js}");
        assert!(js.contains("exports.default = "), "{js}");
    }

    #[test]
    fn exports_become_getters() {
        let js = super::transpile(
            "a.ts",
            "import { x } from './b';\nexport const y = { x };\nexport function f() { return x; }\nexport { x as z };\nexport * from './c';",
        )
        .unwrap();
        assert!(js.contains("get: () => y"), "{js}");
        assert!(js.contains("get: () => f"), "{js}");
        assert!(js.contains("\"z\", { enumerable: true, configurable: true, get: () => __m0[\"x\"]"), "{js}");
        assert!(js.contains("{ x: __m0[\"x\"] }"), "{js}");
        assert!(js.contains("__star(exports, __require(\"./c\"))"), "{js}");
        assert!(!js.contains("export "), "{js}");
    }
}
