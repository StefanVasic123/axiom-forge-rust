//! # AST Engine (Faza 3 — `v0.1.55`)
//!
//! Sintaksni AST sloj zasnovan na `tree-sitter` + `tree-sitter-typescript` grammar-u.
//! Zamenjuje krhke regex heuristike za:
//! - detekciju sintaksnih grešaka (`ERROR` / `MISSING` čvorovi sa tačnom linijom i kolonom),
//! - import / export ugovore,
//! - potpise custom React hook-ova (parametri, podrazumevane vrednosti, inline tipovi, `return { ... }` ključevi),
//! - pozive hook-ova (broj argumenata, destrukturirani ključevi, ključevi objekta-argumenta),
//! - oblike `interface` / `type` deklaracija (cross-file model ugovori).
//!
//! Sve funkcije su čiste (bez I/O), osim `reconcile_hook_contracts_in_project`
//! koja deterministički usklađuje hook ↔ pozivalac ugovore na disku.

use tree_sitter::{Node, Parser, Tree};

/// Maksimalna veličina fajla koju AST engine parsira (zaštita od generisanih bundle-ova).
const MAX_AST_SOURCE_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceDialect {
    TypeScript,
    Tsx,
}

impl SourceDialect {
    pub fn is_typed(&self, path: &str) -> bool {
        let low = path.replace('\\', "/").to_ascii_lowercase();
        low.ends_with(".ts") || low.ends_with(".tsx") || low.ends_with(".mts") || low.ends_with(".cts")
    }
}

/// Bira grammar na osnovu ekstenzije. `.js`/`.jsx` koriste TSX grammar (nadskup JS + JSX).
pub fn dialect_for_path(path: &str) -> Option<SourceDialect> {
    let low = path.replace('\\', "/").to_ascii_lowercase();
    if low.ends_with(".ts") || low.ends_with(".mts") || low.ends_with(".cts") {
        Some(SourceDialect::TypeScript)
    } else if low.ends_with(".tsx")
        || low.ends_with(".jsx")
        || low.ends_with(".js")
        || low.ends_with(".mjs")
        || low.ends_with(".cjs")
    {
        Some(SourceDialect::Tsx)
    } else {
        None
    }
}

pub fn is_typed_path(path: &str) -> bool {
    let low = path.replace('\\', "/").to_ascii_lowercase();
    low.ends_with(".ts") || low.ends_with(".tsx") || low.ends_with(".mts") || low.ends_with(".cts")
}

/// Parsira izvorni kod u `tree_sitter::Tree`. Vraća `None` za nepodržane ekstenzije ili prevelike fajlove.
pub fn parse_source(path: &str, content: &str) -> Option<Tree> {
    if content.len() > MAX_AST_SOURCE_BYTES {
        return None;
    }
    let dialect = dialect_for_path(path)?;
    let mut parser = Parser::new();
    let lang: tree_sitter::Language = match dialect {
        SourceDialect::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        SourceDialect::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
    };
    parser.set_language(&lang).ok()?;
    parser.parse(content, None)
}

// ---------------------------------------------------------------------------
// Pomoćne funkcije
// ---------------------------------------------------------------------------

fn node_text<'a>(node: Node, src: &'a str) -> &'a str {
    src.get(node.start_byte()..node.end_byte()).unwrap_or("")
}

fn named_children_vec<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|c| c.kind() != "comment")
        .collect()
}

fn all_children_vec<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

fn has_token_child(node: Node, token: &str) -> bool {
    all_children_vec(node)
        .into_iter()
        .any(|c| !c.is_named() && c.kind() == token)
}

fn first_named_of_kind<'t>(node: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    named_children_vec(node).into_iter().find(|c| kinds.contains(&c.kind()))
}

fn strip_string_quotes(raw: &str) -> String {
    raw.trim()
        .trim_start_matches(|c| c == '\'' || c == '"' || c == '`')
        .trim_end_matches(|c| c == '\'' || c == '"' || c == '`')
        .to_string()
}

fn property_key_name(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "property_identifier" | "identifier" | "shorthand_property_identifier"
        | "shorthand_property_identifier_pattern" | "private_property_identifier" | "type_identifier" => {
            Some(node_text(node, src).to_string())
        }
        "string" => Some(strip_string_quotes(node_text(node, src))),
        "number" => Some(node_text(node, src).to_string()),
        _ => None,
    }
}

fn is_function_like(kind: &str) -> bool {
    matches!(
        kind,
        "arrow_function"
            | "function_expression"
            | "function"
            | "function_declaration"
            | "generator_function"
            | "generator_function_declaration"
            | "method_definition"
            | "class_declaration"
            | "class"
    )
}

fn is_hook_name(name: &str) -> bool {
    let mut chars = name.chars();
    name.starts_with("use")
        && name.len() > 3
        && chars.nth(3).map_or(false, |c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn unwrap_expression<'t>(mut node: Node<'t>) -> Node<'t> {
    loop {
        match node.kind() {
            "parenthesized_expression" | "await_expression" | "as_expression" | "satisfies_expression"
            | "non_null_expression" => {
                if let Some(inner) = named_children_vec(node).into_iter().next() {
                    node = inner;
                    continue;
                }
                return node;
            }
            _ => return node,
        }
    }
}

// ---------------------------------------------------------------------------
// 1. Sintaksni integritet
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxIssueKind {
    /// Parser nije mogao da prepozna deo koda (`ERROR` čvor).
    Unexpected,
    /// Parser je morao da „zamisli“ token koji fali (npr. `}` ili `)`).
    Missing(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxIssue {
    /// 1-based linija
    pub line: usize,
    /// 1-based kolona
    pub column: usize,
    pub kind: SyntaxIssueKind,
    pub snippet: String,
}

impl SyntaxIssue {
    pub fn describe(&self) -> String {
        match &self.kind {
            SyntaxIssueKind::Unexpected => format!(
                "Line {}, Col {}: unexpected/unparseable code near `{}`",
                self.line, self.column, self.snippet
            ),
            SyntaxIssueKind::Missing(tok) => format!(
                "Line {}, Col {}: missing `{}` near `{}`",
                self.line, self.column, tok, self.snippet
            ),
        }
    }
}

fn snippet_around(src: &str, start: usize, end: usize) -> String {
    let s = start.min(src.len());
    let e = end.max(s).min(src.len());
    let raw = src.get(s..e).unwrap_or("");
    let first_line = raw.lines().next().unwrap_or("").trim();
    let source = if first_line.is_empty() {
        // Za MISSING čvorove (prazni) prikaži liniju u kojoj se nalaze
        let line_start = src[..s].rfind('\n').map_or(0, |p| p + 1);
        let line_end = src[s..].find('\n').map_or(src.len(), |p| s + p);
        src.get(line_start..line_end).unwrap_or("").trim().to_string()
    } else {
        first_line.to_string()
    };
    source.chars().take(80).collect()
}

pub fn collect_syntax_issues(tree: &Tree, src: &str, limit: usize) -> Vec<SyntaxIssue> {
    let mut issues = Vec::new();
    let root = tree.root_node();
    if !root.has_error() {
        return issues;
    }
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if issues.len() >= limit {
            break;
        }
        if node.is_error() {
            let pos = node.start_position();
            issues.push(SyntaxIssue {
                line: pos.row + 1,
                column: pos.column + 1,
                kind: SyntaxIssueKind::Unexpected,
                snippet: snippet_around(src, node.start_byte(), node.end_byte()),
            });
            continue;
        }
        if node.is_missing() {
            let pos = node.start_position();
            issues.push(SyntaxIssue {
                line: pos.row + 1,
                column: pos.column + 1,
                kind: SyntaxIssueKind::Missing(node.kind().to_string()),
                snippet: snippet_around(src, node.start_byte(), node.end_byte()),
            });
            continue;
        }
        if node.has_error() {
            let mut children = all_children_vec(node);
            children.reverse();
            stack.extend(children);
        }
    }
    issues.sort_by_key(|i| (i.line, i.column));
    issues
}

/// Brza provera: vraća listu sintaksnih problema (prazna lista = sintaksno ispravan fajl).
pub fn syntax_issues_for(path: &str, content: &str) -> Option<Vec<SyntaxIssue>> {
    let tree = parse_source(path, content)?;
    Some(collect_syntax_issues(&tree, content, 8))
}

// ---------------------------------------------------------------------------
// 2. Import / Export ugovori
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportBinding {
    pub imported: String,
    pub local: String,
    pub type_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportDecl {
    pub source: String,
    pub default_local: Option<String>,
    pub namespace_local: Option<String>,
    pub named: Vec<ImportBinding>,
    pub type_only: bool,
    pub line: usize,
}

pub fn extract_imports(tree: &Tree, src: &str) -> Vec<ImportDecl> {
    let mut out = Vec::new();
    for stmt in named_children_vec(tree.root_node()) {
        if stmt.kind() != "import_statement" {
            continue;
        }
        let mut decl = ImportDecl {
            line: stmt.start_position().row + 1,
            type_only: has_token_child(stmt, "type"),
            ..Default::default()
        };
        if let Some(source) = stmt
            .child_by_field_name("source")
            .or_else(|| first_named_of_kind(stmt, &["string"]))
        {
            decl.source = strip_string_quotes(node_text(source, src));
        }
        if let Some(clause) = first_named_of_kind(stmt, &["import_clause"]) {
            for part in named_children_vec(clause) {
                match part.kind() {
                    "identifier" => decl.default_local = Some(node_text(part, src).to_string()),
                    "namespace_import" => {
                        if let Some(id) = first_named_of_kind(part, &["identifier"]) {
                            decl.namespace_local = Some(node_text(id, src).to_string());
                        }
                    }
                    "named_imports" => {
                        for spec in named_children_vec(part) {
                            if spec.kind() != "import_specifier" {
                                continue;
                            }
                            let name_node = spec
                                .child_by_field_name("name")
                                .or_else(|| named_children_vec(spec).into_iter().next());
                            let alias_node = spec.child_by_field_name("alias");
                            if let Some(n) = name_node {
                                let imported = strip_string_quotes(node_text(n, src));
                                let local = alias_node
                                    .map(|a| node_text(a, src).to_string())
                                    .unwrap_or_else(|| imported.clone());
                                decl.named.push(ImportBinding {
                                    imported,
                                    local,
                                    type_only: decl.type_only || has_token_child(spec, "type"),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        out.push(decl);
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExportSummary {
    /// Imenovani eksporti (uključujući `export { a as b }` → `b`)
    pub named: Vec<String>,
    pub has_default: bool,
    /// Ime lokalnog simbola koji je default eksport (ako je poznato)
    pub default_local: Option<String>,
    /// `export * from '...'`
    pub star_reexports: Vec<String>,
}

fn declaration_names(decl: Node, src: &str) -> Vec<String> {
    match decl.kind() {
        "function_declaration" | "generator_function_declaration" | "class_declaration"
        | "abstract_class_declaration" | "interface_declaration" | "type_alias_declaration"
        | "enum_declaration" | "function_signature" => decl
            .child_by_field_name("name")
            .map(|n| vec![node_text(n, src).to_string()])
            .unwrap_or_default(),
        "lexical_declaration" | "variable_declaration" => named_children_vec(decl)
            .into_iter()
            .filter(|c| c.kind() == "variable_declarator")
            .filter_map(|d| d.child_by_field_name("name"))
            .filter(|n| n.kind() == "identifier")
            .map(|n| node_text(n, src).to_string())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn extract_exports(tree: &Tree, src: &str) -> ExportSummary {
    let mut out = ExportSummary::default();
    let push_unique = |list: &mut Vec<String>, v: String| {
        if !v.is_empty() && !list.contains(&v) {
            list.push(v);
        }
    };
    for stmt in named_children_vec(tree.root_node()) {
        if stmt.kind() != "export_statement" {
            continue;
        }
        let is_default = has_token_child(stmt, "default");
        let source = stmt.child_by_field_name("source");

        if let Some(decl) = stmt.child_by_field_name("declaration") {
            let names = declaration_names(decl, src);
            if is_default {
                out.has_default = true;
                out.default_local = names.first().cloned();
            } else {
                for n in names {
                    push_unique(&mut out.named, n);
                }
            }
            continue;
        }

        if is_default {
            out.has_default = true;
            if let Some(val) = stmt.child_by_field_name("value") {
                let inner = unwrap_expression(val);
                if inner.kind() == "identifier" {
                    out.default_local = Some(node_text(inner, src).to_string());
                } else if let Some(n) = inner.child_by_field_name("name") {
                    out.default_local = Some(node_text(n, src).to_string());
                }
            } else {
                for c in named_children_vec(stmt) {
                    let names = declaration_names(c, src);
                    if let Some(first) = names.into_iter().next() {
                        out.default_local = Some(first);
                        break;
                    }
                    if c.kind() == "identifier" {
                        out.default_local = Some(node_text(c, src).to_string());
                        break;
                    }
                }
            }
            continue;
        }

        if let Some(clause) = first_named_of_kind(stmt, &["export_clause"]) {
            for spec in named_children_vec(clause) {
                if spec.kind() != "export_specifier" {
                    continue;
                }
                let name = spec
                    .child_by_field_name("name")
                    .map(|n| strip_string_quotes(node_text(n, src)));
                let alias = spec
                    .child_by_field_name("alias")
                    .map(|n| strip_string_quotes(node_text(n, src)));
                let exported = alias.or(name).unwrap_or_default();
                if exported == "default" {
                    out.has_default = true;
                } else {
                    push_unique(&mut out.named, exported);
                }
            }
            continue;
        }

        if let Some(ns) = first_named_of_kind(stmt, &["namespace_export"]) {
            if let Some(id) = named_children_vec(ns).into_iter().last() {
                push_unique(&mut out.named, strip_string_quotes(node_text(id, src)));
            }
            continue;
        }

        if has_token_child(stmt, "*") {
            if let Some(s) = source {
                push_unique(&mut out.star_reexports, strip_string_quotes(node_text(s, src)));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 3. Oblici tipova (interface / type) — cross-file model ugovori
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeProperty {
    pub name: String,
    pub optional: bool,
    pub type_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeShape {
    pub name: String,
    pub kind: &'static str,
    pub exported: bool,
    pub properties: Vec<TypeProperty>,
}

fn collect_object_type_properties(body: Node, src: &str) -> Vec<TypeProperty> {
    let mut props = Vec::new();
    for member in named_children_vec(body) {
        if !matches!(member.kind(), "property_signature" | "method_signature") {
            continue;
        }
        let Some(name_node) = member.child_by_field_name("name") else { continue };
        let Some(name) = property_key_name(name_node, src) else { continue };
        let type_text = member
            .child_by_field_name("type")
            .map(|t| node_text(t, src).trim_start_matches(':').trim().to_string())
            .unwrap_or_else(|| {
                if member.kind() == "method_signature" {
                    "function".to_string()
                } else {
                    String::new()
                }
            });
        props.push(TypeProperty {
            name,
            optional: has_token_child(member, "?"),
            type_text,
        });
    }
    props
}

pub fn extract_type_shapes(tree: &Tree, src: &str) -> Vec<TypeShape> {
    let mut shapes = Vec::new();
    for stmt in named_children_vec(tree.root_node()) {
        let (decl, exported) = if stmt.kind() == "export_statement" {
            match stmt.child_by_field_name("declaration") {
                Some(d) => (d, true),
                None => continue,
            }
        } else {
            (stmt, false)
        };
        match decl.kind() {
            "interface_declaration" => {
                let Some(name) = decl.child_by_field_name("name") else { continue };
                let props = decl
                    .child_by_field_name("body")
                    .map(|b| collect_object_type_properties(b, src))
                    .unwrap_or_default();
                shapes.push(TypeShape {
                    name: node_text(name, src).to_string(),
                    kind: "interface",
                    exported,
                    properties: props,
                });
            }
            "type_alias_declaration" => {
                let Some(name) = decl.child_by_field_name("name") else { continue };
                let props = decl
                    .child_by_field_name("value")
                    .filter(|v| v.kind() == "object_type")
                    .map(|v| collect_object_type_properties(v, src))
                    .unwrap_or_default();
                shapes.push(TypeShape {
                    name: node_text(name, src).to_string(),
                    kind: "type",
                    exported,
                    properties: props,
                });
            }
            _ => {}
        }
    }
    shapes
}

// ---------------------------------------------------------------------------
// 4. Hook potpisi i pozivi
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookParamKind {
    ObjectPattern,
    Identifier,
    Rest,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineTypeProp {
    pub name: String,
    pub optional: bool,
    /// Bajt odmah posle imena svojstva (mesto za ubacivanje `?`)
    pub name_end_byte: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookParam {
    pub kind: HookParamKind,
    pub start_byte: usize,
    pub end_byte: usize,
    /// Bajt kraja samog pattern-a (pre `: Type`)
    pub pattern_end_byte: usize,
    pub type_end_byte: Option<usize>,
    pub type_already_any: bool,
    pub has_default: bool,
    pub optional: bool,
    pub has_type_annotation: bool,
    pub object_keys: Vec<String>,
    pub inline_type_props: Vec<InlineTypeProp>,
}

impl HookParam {
    pub fn is_satisfiable_without_arg(&self) -> bool {
        self.has_default || self.optional || self.kind == HookParamKind::Rest
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnObject {
    pub keys: Vec<String>,
    pub has_spread: bool,
    /// Pozicija za ubacivanje novog svojstva
    pub insert_byte: usize,
    /// Da li ubačenom svojstvu treba vodeći zarez
    pub needs_leading_comma: bool,
    /// Da li je objekat prazan `{}`
    pub is_empty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookSignature {
    pub name: String,
    pub line: usize,
    pub params: Vec<HookParam>,
    /// Pozicija zatvarajuće `)` liste parametara (ako postoji)
    pub params_close_byte: Option<usize>,
    pub params_trailing_comma: bool,
    pub return_objects: Vec<ReturnObject>,
    pub has_explicit_return_type: bool,
    pub explicit_return_type_name: Option<String>,
    pub return_type_end_byte: Option<usize>,
    pub return_type_already_any: bool,
}

fn analyze_param(param: Node, src: &str) -> HookParam {
    let kind_str = param.kind();
    let optional = kind_str == "optional_parameter";
    let pattern = param
        .child_by_field_name("pattern")
        .or_else(|| named_children_vec(param).into_iter().next())
        .unwrap_or(param);
    let has_default = param.child_by_field_name("value").is_some();
    let type_ann = param.child_by_field_name("type");

    let kind = match pattern.kind() {
        "object_pattern" => HookParamKind::ObjectPattern,
        "identifier" => HookParamKind::Identifier,
        "rest_pattern" => HookParamKind::Rest,
        _ => HookParamKind::Other,
    };
    let mut object_keys = Vec::new();
    if kind == HookParamKind::ObjectPattern {
        object_keys = object_pattern_keys(pattern, src);
    }
    let mut inline_type_props = Vec::new();
    if let Some(ta) = type_ann {
        if let Some(obj_ty) = first_named_of_kind(ta, &["object_type"]) {
            for member in named_children_vec(obj_ty) {
                if member.kind() != "property_signature" {
                    continue;
                }
                if let Some(name_node) = member.child_by_field_name("name") {
                    if let Some(name) = property_key_name(name_node, src) {
                        inline_type_props.push(InlineTypeProp {
                            name,
                            optional: has_token_child(member, "?"),
                            name_end_byte: name_node.end_byte(),
                        });
                    }
                }
            }
        }
    }
    let type_end_byte = type_ann.map(|ta| ta.end_byte());
    let type_already_any = type_ann
        .map(|ta| node_text(ta, src).contains("any"))
        .unwrap_or(false);
    HookParam {
        kind,
        start_byte: param.start_byte(),
        end_byte: param.end_byte(),
        pattern_end_byte: pattern.end_byte(),
        type_end_byte,
        type_already_any,
        has_default,
        optional,
        has_type_annotation: type_ann.is_some(),
        object_keys,
        inline_type_props,
    }
}

fn object_pattern_keys(pattern: Node, src: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for child in named_children_vec(pattern) {
        let key = match child.kind() {
            "shorthand_property_identifier_pattern" => Some(node_text(child, src).to_string()),
            "pair_pattern" => child
                .child_by_field_name("key")
                .and_then(|k| property_key_name(k, src)),
            "object_assignment_pattern" => child
                .child_by_field_name("left")
                .and_then(|l| property_key_name(l, src)),
            _ => None,
        };
        if let Some(k) = key {
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    keys
}

fn analyze_object_literal(obj: Node, src: &str) -> ReturnObject {
    let mut keys = Vec::new();
    let mut has_spread = false;
    let mut last_named_end: Option<usize> = None;
    for child in named_children_vec(obj) {
        match child.kind() {
            "pair" => {
                if let Some(k) = child.child_by_field_name("key").and_then(|k| property_key_name(k, src)) {
                    keys.push(k);
                }
            }
            "shorthand_property_identifier" => keys.push(node_text(child, src).to_string()),
            "method_definition" => {
                if let Some(k) = child.child_by_field_name("name").and_then(|k| property_key_name(k, src)) {
                    keys.push(k);
                }
            }
            "spread_element" => has_spread = true,
            _ => {}
        }
        last_named_end = Some(child.end_byte());
    }
    match last_named_end {
        None => ReturnObject {
            keys,
            has_spread,
            insert_byte: obj.start_byte() + 1,
            needs_leading_comma: false,
            is_empty: true,
        },
        Some(end) => {
            let trailing_comma = all_children_vec(obj)
                .into_iter()
                .find(|c| !c.is_named() && c.kind() == "," && c.start_byte() >= end);
            match trailing_comma {
                Some(comma) => ReturnObject {
                    keys,
                    has_spread,
                    insert_byte: comma.end_byte(),
                    needs_leading_comma: false,
                    is_empty: false,
                },
                None => ReturnObject {
                    keys,
                    has_spread,
                    insert_byte: end,
                    needs_leading_comma: true,
                    is_empty: false,
                },
            }
        }
    }
}

fn collect_return_objects(body: Node, src: &str, out: &mut Vec<ReturnObject>) {
    // Telo strelice koje je direktno izraz: `() => ({ ... })`
    if body.kind() != "statement_block" {
        let inner = unwrap_expression(body);
        if inner.kind() == "object" {
            out.push(analyze_object_literal(inner, src));
        }
        return;
    }
    let mut stack: Vec<Node> = named_children_vec(body);
    while let Some(node) = stack.pop() {
        if is_function_like(node.kind()) {
            continue;
        }
        if node.kind() == "return_statement" {
            if let Some(expr) = named_children_vec(node).into_iter().next() {
                let inner = unwrap_expression(expr);
                if inner.kind() == "object" {
                    out.push(analyze_object_literal(inner, src));
                }
            }
            continue;
        }
        stack.extend(named_children_vec(node));
    }
}

fn is_valid_js_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn type_ident_or_generic_name(node: Node, src: &str) -> Option<String> {
    if node.kind() == "type_identifier" {
        return Some(node_text(node, src).to_string());
    }
    if node.kind() == "generic_type" {
        if let Some(id) = first_named_of_kind(node, &["type_identifier"]) {
            return Some(node_text(id, src).to_string());
        }
    }
    None
}

fn extract_explicit_return_type_name(func: Node, src: &str) -> Option<String> {
    let ret_ann = func.child_by_field_name("return_type")?;
    let named = named_children_vec(ret_ann);
    if named.len() == 1 {
        if let Some(n) = type_ident_or_generic_name(named[0], src) {
            return Some(n);
        }
    }
    type_ident_or_generic_name(ret_ann, src)
}

fn extract_var_fn_return_type_name(declarator: Node, src: &str) -> Option<String> {
    let type_ann = declarator.child_by_field_name("type")?;
    let mut stack = named_children_vec(type_ann);
    while let Some(n) = stack.pop() {
        if n.kind() == "function_type" {
            if let Some(rt) = n.child_by_field_name("return_type") {
                if let Some(name) = type_ident_or_generic_name(rt, src) {
                    return Some(name);
                }
                let inner = named_children_vec(rt);
                if inner.len() == 1 {
                    if let Some(name) = type_ident_or_generic_name(inner[0], src) {
                        return Some(name);
                    }
                }
            }
        }
        stack.extend(named_children_vec(n));
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTypeContract {
    pub name: String,
    pub existing_keys: Vec<String>,
    pub insert_byte: usize,
    pub has_index_sig: bool,
    pub required_prop_name_ends: Vec<usize>,
}

/// Izvlači top-level `interface X { ... }` i objektne `type X = { ... }` deklaracije u istom fajlu.
pub fn extract_local_type_contracts(tree: &Tree, src: &str) -> Vec<LocalTypeContract> {
    let mut contracts = Vec::new();
    let make_contract = |name: String, body: Node| -> Option<LocalTypeContract> {
        let end = body.end_byte();
        if end == 0 || src.as_bytes().get(end - 1) != Some(&b'}') {
            return None;
        }
        let existing_keys: Vec<String> = collect_object_type_properties(body, src)
            .into_iter()
            .map(|p| p.name)
            .collect();
        let members = named_children_vec(body);
        let has_index_sig = members.iter().any(|m| m.kind() == "index_signature")
            || node_text(body, src).contains("[key: string]");
        let mut required_prop_name_ends = Vec::new();
        for member in members {
            if member.kind() == "property_signature" && !has_token_child(member, "?") {
                if let Some(nn) = member.child_by_field_name("name") {
                    required_prop_name_ends.push(nn.end_byte());
                }
            }
        }
        Some(LocalTypeContract {
            name,
            existing_keys,
            insert_byte: end - 1,
            has_index_sig,
            required_prop_name_ends,
        })
    };

    for stmt in named_children_vec(tree.root_node()) {
        let decl = if stmt.kind() == "export_statement" {
            match stmt.child_by_field_name("declaration") {
                Some(d) => d,
                None => continue,
            }
        } else {
            stmt
        };
        match decl.kind() {
            "interface_declaration" => {
                let Some(name_node) = decl.child_by_field_name("name") else { continue };
                let Some(body) = decl.child_by_field_name("body") else { continue };
                if let Some(c) = make_contract(node_text(name_node, src).to_string(), body) {
                    contracts.push(c);
                }
            }
            "type_alias_declaration" => {
                let Some(name_node) = decl.child_by_field_name("name") else { continue };
                let Some(val) = decl.child_by_field_name("value") else { continue };
                if val.kind() == "object_type" {
                    if let Some(c) = make_contract(node_text(name_node, src).to_string(), val) {
                        contracts.push(c);
                    }
                }
            }
            _ => {}
        }
    }
    contracts
}

fn build_hook_signature(name: &str, decl_node: Node, func: Node, var_ret_type: Option<String>, src: &str) -> HookSignature {
    let mut params = Vec::new();
    let mut params_close_byte = None;
    let mut params_trailing_comma = false;
    if let Some(fp) = func.child_by_field_name("parameters") {
        for p in named_children_vec(fp) {
            if matches!(p.kind(), "required_parameter" | "optional_parameter") {
                params.push(analyze_param(p, src));
            } else if matches!(p.kind(), "identifier" | "object_pattern" | "rest_pattern") {
                params.push(analyze_param(p, src));
            }
        }
        let tokens = all_children_vec(fp);
        if let Some(close) = tokens.iter().rev().find(|c| !c.is_named() && c.kind() == ")") {
            params_close_byte = Some(close.start_byte());
            let last_param_end = params.last().map(|p| p.end_byte).unwrap_or(fp.start_byte());
            params_trailing_comma = tokens
                .iter()
                .any(|c| !c.is_named() && c.kind() == "," && c.start_byte() >= last_param_end && c.start_byte() < close.start_byte());
        }
    } else if let Some(single) = func.child_by_field_name("parameter") {
        params.push(analyze_param(single, src));
    }
    let mut return_objects = Vec::new();
    if let Some(body) = func.child_by_field_name("body") {
        collect_return_objects(body, src, &mut return_objects);
    }
    let explicit_return_type_name = extract_explicit_return_type_name(func, src).or(var_ret_type);
    let ret_ann_node = func.child_by_field_name("return_type");
    let return_type_end_byte = ret_ann_node.map(|r| r.end_byte());
    let return_type_already_any = ret_ann_node
        .map(|r| node_text(r, src).contains("any"))
        .unwrap_or(false);
    HookSignature {
        name: name.to_string(),
        line: decl_node.start_position().row + 1,
        params,
        params_close_byte,
        params_trailing_comma,
        return_objects,
        has_explicit_return_type: ret_ann_node.is_some() || explicit_return_type_name.is_some(),
        explicit_return_type_name,
        return_type_end_byte,
        return_type_already_any,
    }
}

/// Pronalazi top-level definicije custom hook-ova (`function useX`, `const useX = (...) =>`, `export ...`).
pub fn extract_hook_signatures(tree: &Tree, src: &str) -> Vec<HookSignature> {
    let mut hooks = Vec::new();
    for stmt in named_children_vec(tree.root_node()) {
        let decl = if stmt.kind() == "export_statement" {
            match stmt.child_by_field_name("declaration") {
                Some(d) => d,
                None => continue,
            }
        } else {
            stmt
        };
        match decl.kind() {
            "function_declaration" | "generator_function_declaration" => {
                if let Some(n) = decl.child_by_field_name("name") {
                    let name = node_text(n, src);
                    if is_hook_name(name) {
                        hooks.push(build_hook_signature(name, decl, decl, None, src));
                    }
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                for d in named_children_vec(decl) {
                    if d.kind() != "variable_declarator" {
                        continue;
                    }
                    let (Some(n), Some(v)) = (d.child_by_field_name("name"), d.child_by_field_name("value")) else {
                        continue;
                    };
                    let name = node_text(n, src);
                    let func = unwrap_expression(v);
                    if is_hook_name(name)
                        && matches!(func.kind(), "arrow_function" | "function_expression" | "function")
                    {
                        let var_ret = extract_var_fn_return_type_name(d, src);
                        hooks.push(build_hook_signature(name, decl, func, var_ret, src));
                    }
                }
            }
            _ => {}
        }
    }
    hooks
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookCallSite {
    pub hook_name: String,
    pub line: usize,
    pub arg_count: usize,
    /// Ključevi destrukturirani iz rezultata: `const { data, error } = useX()`
    pub destructured_keys: Vec<String>,
    /// Da li pozivalac destrukturira povratnu vrednost kao niz (`const [a, b] = useX()`)
    pub destructured_as_array: bool,
    /// Ključevi prvog argumenta ako je objekat-literal: `useX({ userId })`
    pub object_arg_keys: Option<Vec<String>>,
}

pub fn extract_hook_call_sites(tree: &Tree, src: &str) -> Vec<HookCallSite> {
    let mut calls = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                if func.kind() == "identifier" && is_hook_name(node_text(func, src)) {
                    let args = node
                        .child_by_field_name("arguments")
                        .map(named_children_vec)
                        .unwrap_or_default();
                    let object_arg_keys = args.first().and_then(|a| {
                        let inner = unwrap_expression(*a);
                        if inner.kind() == "object" {
                            Some(analyze_object_literal(inner, src).keys)
                        } else {
                            None
                        }
                    });
                    // Penji se kroz `await` / `as` / zagrade do `variable_declarator`
                    let mut destructured_keys = Vec::new();
                    let mut destructured_as_array = false;
                    let mut cur = node;
                    while let Some(parent) = cur.parent() {
                        match parent.kind() {
                            "await_expression" | "parenthesized_expression" | "as_expression"
                            | "non_null_expression" | "satisfies_expression" => {
                                cur = parent;
                            }
                            "variable_declarator" => {
                                if let Some(name) = parent.child_by_field_name("name") {
                                    if name.kind() == "object_pattern" {
                                        destructured_keys = object_pattern_keys(name, src);
                                    } else if name.kind() == "array_pattern" {
                                        destructured_as_array = true;
                                    }
                                }
                                break;
                            }
                            _ => break,
                        }
                    }
                    calls.push(HookCallSite {
                        hook_name: node_text(func, src).to_string(),
                        line: node.start_position().row + 1,
                        arg_count: args.len(),
                        destructured_keys,
                        destructured_as_array,
                        object_arg_keys,
                    });
                }
            }
        }
        let mut children = named_children_vec(node);
        children.reverse();
        stack.extend(children);
    }
    calls.sort_by_key(|c| c.line);
    calls
}

// ---------------------------------------------------------------------------
// 5. Determinističke AST izmene (insertion-only, re-parse verifikovane)
// ---------------------------------------------------------------------------

/// Primenjuje listu ubacivanja `(bajt, tekst)` od kraja ka početku.
pub fn apply_insertions(src: &str, mut insertions: Vec<(usize, String)>) -> String {
    // Stabilno sortiranje: isti bajt zadržava redosled navođenja
    let indexed: Vec<(usize, usize, String)> = insertions
        .drain(..)
        .enumerate()
        .map(|(i, (pos, text))| (pos, i, text))
        .collect();
    let mut sorted = indexed;
    sorted.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let mut out = src.to_string();
    for (pos, _, text) in sorted {
        if pos <= out.len() && out.is_char_boundary(pos) {
            out.insert_str(pos, &text);
        }
    }
    out
}

/// Vraća `Some(novi_kod)` samo ako izmena ne uvodi nove sintaksne greške.
pub fn verified_rewrite(path: &str, original: &str, candidate: String) -> Option<String> {
    if candidate == original {
        return None;
    }
    let before = parse_source(path, original).map(|t| t.root_node().has_error()).unwrap_or(true);
    let after_tree = parse_source(path, &candidate)?;
    if after_tree.root_node().has_error() && !before {
        return None;
    }
    Some(candidate)
}

/// Dodaje bezbedne podrazumevane vrednosti destrukturiranim objekat-parametrima hook-ova
/// (`({ a }: Props)` → `({ a }: Props = {} as any)`) kad svi naredni parametri mogu da izostanu.
/// Vraća `None` ako fajl ne može da se parsira (pozivalac koristi fallback).
pub fn add_hook_object_param_defaults(path: &str, content: &str) -> Option<String> {
    let tree = parse_source(path, content)?;
    let typed = is_typed_path(path);
    let default_text = if typed { " = {} as any" } else { " = {}" };
    let mut insertions = Vec::new();
    for hook in extract_hook_signatures(&tree, content) {
        for (idx, p) in hook.params.iter().enumerate() {
            if p.kind != HookParamKind::ObjectPattern || p.is_satisfiable_without_arg() {
                continue;
            }
            let rest_ok = hook.params[idx + 1..].iter().all(|q| q.is_satisfiable_without_arg());
            if rest_ok {
                insertions.push((p.end_byte, default_text.to_string()));
            }
        }
    }
    if insertions.is_empty() {
        return Some(content.to_string());
    }
    let candidate = apply_insertions(content, insertions);
    Some(verified_rewrite(path, content, candidate).unwrap_or_else(|| content.to_string()))
}

/// Podrazumevana vrednost za ključ koji pozivalac očekuje, a hook ga ne vraća.
pub fn default_value_for_missing_hook_key(key: &str, typed: bool) -> String {
    let any = |v: &str| if typed { format!("{} as any", v) } else { v.to_string() };
    let low = key.to_ascii_lowercase();
    let starts_upper_after = |prefix: &str| {
        key.len() > prefix.len()
            && key.starts_with(prefix)
            && key[prefix.len()..].chars().next().map_or(false, |c| c.is_ascii_uppercase())
    };
    if matches!(low.as_str(), "error" | "err" | "errormessage" | "fetcherror" | "loaderror") || low.ends_with("error") {
        return any("null");
    }
    if matches!(low.as_str(), "loading" | "isloading" | "isfetching" | "iserror" | "issuccess" | "isvalidating" | "isrefreshing" | "ispending")
        || starts_upper_after("is")
        || starts_upper_after("has")
        || starts_upper_after("can")
    {
        return "false".to_string();
    }
    if matches!(
        low.as_str(),
        "refetch" | "reload" | "refresh" | "mutate" | "retry" | "reset" | "invalidate" | "fetchmore" | "loadmore"
    ) || starts_upper_after("fetch")
        || starts_upper_after("load")
        || starts_upper_after("refresh")
    {
        return if typed {
            "async (..._args: any[]) => {}".to_string()
        } else {
            "async () => {}".to_string()
        };
    }
    if starts_upper_after("set") || starts_upper_after("on") || starts_upper_after("handle")
        || starts_upper_after("create") || starts_upper_after("update") || starts_upper_after("delete")
        || starts_upper_after("remove") || starts_upper_after("add") || starts_upper_after("toggle")
    {
        return if typed {
            "(..._args: any[]) => {}".to_string()
        } else {
            "() => {}".to_string()
        };
    }
    if matches!(low.as_str(), "count" | "total" | "page" | "totalpages" | "totalcount") {
        return "0".to_string();
    }
    any("undefined")
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookContractReport {
    pub edited_files: Vec<String>,
    pub actions: Vec<String>,
}

struct ParsedProjectFile {
    rel: String,
    abs: std::path::PathBuf,
    content: String,
    hooks: Vec<HookSignature>,
    local_contracts: Vec<LocalTypeContract>,
    calls: Vec<HookCallSite>,
}

fn collect_source_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if !matches!(
                name,
                "node_modules" | ".next" | ".git" | ".axiom" | "dist" | "build" | "prisma" | "public" | "coverage"
            ) {
                collect_source_files(&p, out);
            }
        } else if p.is_file()
            && dialect_for_path(name).is_some()
            && !name.ends_with(".d.ts")
            && !name.ends_with(".config.js")
            && !name.ends_with(".config.ts")
            && !name.ends_with(".config.mjs")
        {
            out.push(p);
        }
    }
}

/// Planira AST izmene jednog hook fajla na osnovu svih poziva hook-a u projektu
/// i lokalne `interface`/`type` deklaracije eksplicitnog povratnog tipa (Faza 4).
pub fn plan_hook_and_interface_insertions(
    rel_path: &str,
    hook: &HookSignature,
    local_contracts: &[LocalTypeContract],
    calls: &[&HookCallSite],
    actions: &mut Vec<String>,
) -> Vec<(usize, String)> {
    let typed = is_typed_path(rel_path);
    let mut insertions: Vec<(usize, String)> = Vec::new();

    let mut demanded: Vec<String> = Vec::new();
    for c in calls {
        for k in &c.destructured_keys {
            if is_valid_js_identifier(k) && !demanded.contains(k) {
                demanded.push(k.clone());
            }
        }
    }

    // (a0) Eksplicitni return tip (`const useAuth = (): AuthState =>`) usklađuje se aditivno
    // (Set-Difference samo za missing ključeve, uvek `prop?: any;`)
    let matched_contract = hook
        .explicit_return_type_name
        .as_ref()
        .and_then(|tname| local_contracts.iter().find(|c| &c.name == tname));

    if let Some(contract) = matched_contract {
        let mut required_iface_keys: Vec<String> = Vec::new();
        for ret in &hook.return_objects {
            for k in &ret.keys {
                if is_valid_js_identifier(k) && !required_iface_keys.contains(k) {
                    required_iface_keys.push(k.clone());
                }
            }
        }
        for k in &demanded {
            if !required_iface_keys.contains(k) {
                required_iface_keys.push(k.clone());
            }
        }
        let missing_in_iface: Vec<&String> = required_iface_keys
            .iter()
            .filter(|k| !contract.existing_keys.contains(k))
            .collect();
        let mut block_parts: Vec<String> = Vec::new();
        if !missing_in_iface.is_empty() {
            let decls = missing_in_iface
                .iter()
                .map(|k| format!("  {}?: any;", k))
                .collect::<Vec<_>>()
                .join("\n");
            block_parts.push(decls);
            actions.push(format!(
                "{}: `{}` povratni tip `{}` dopunjen sa {}?: any",
                rel_path,
                hook.name,
                contract.name,
                missing_in_iface.iter().map(|k| format!("`{}`", k)).collect::<Vec<_>>().join(", ")
            ));
        }
        if typed && !contract.has_index_sig {
            block_parts.push("  [key: string]: any;".to_string());
            actions.push(format!(
                "{}: `{}` povratni tip `{}` dopunjen sa `[key: string]: any;`",
                rel_path, hook.name, contract.name
            ));
        }
        if !block_parts.is_empty() {
            insertions.push((contract.insert_byte, format!("\n{}\n", block_parts.join("\n"))));
        }
    }

    // (a0-opts) Lokalne `*Options` / `*Params` / `*Config` / `*Props` deklaracije u hook fajlu dobijaju `[key: string]: any;`
    if typed {
        for c in local_contracts {
            if matched_contract.map_or(false, |mc| mc.insert_byte == c.insert_byte) {
                continue;
            }
            let is_options_like = c.name.ends_with("Options")
                || c.name.ends_with("Params")
                || c.name.ends_with("Config")
                || c.name.ends_with("Props");
            if is_options_like && !c.has_index_sig {
                insertions.push((c.insert_byte, "\n  [key: string]: any;\n".to_string()));
                for &req_end in &c.required_prop_name_ends {
                    insertions.push((req_end, "?".to_string()));
                }
            }
        }
    }

    if calls.is_empty() {
        return insertions;
    }

    // (a-arr) Ako pozivalac destrukturira povratnu vrednost kao niz (`const [a, b] = useX()`),
    //         proširi eksplicitni povratni tip sa `| any` kako bi i tuple i objekat destrukturiranje radili.
    if typed && !hook.return_type_already_any && calls.iter().any(|c| c.destructured_as_array) {
        if let Some(rt_end) = hook.return_type_end_byte {
            insertions.push((rt_end, " | any".to_string()));
        }
    }

    // (a) Ključevi koje pozivaoci destrukturiraju a hook ih ne vraća
    let hook_param_keys: Vec<String> = hook
        .params
        .iter()
        .flat_map(|p| p.object_keys.iter().cloned())
        .collect();
    let can_sync_return_objects = !hook.has_explicit_return_type || matched_contract.is_some();
    if can_sync_return_objects && !hook.return_objects.is_empty() && hook.return_objects.iter().all(|r| !r.has_spread) {
        for ret in &hook.return_objects {
            let missing: Vec<&String> = demanded
                .iter()
                .filter(|k| !ret.keys.contains(k) && !hook_param_keys.contains(k))
                .collect();
            if missing.is_empty() {
                continue;
            }
            let parts: Vec<String> = missing
                .iter()
                .map(|k| format!("{}: {}", k, default_value_for_missing_hook_key(k, typed)))
                .collect();
            let text = if ret.is_empty {
                format!(" {} ", parts.join(", "))
            } else if ret.needs_leading_comma {
                format!(", {}", parts.join(", "))
            } else {
                format!(" {},", parts.join(", "))
            };
            insertions.push((ret.insert_byte, text));
            actions.push(format!(
                "{}: `{}` sada vraća {} (traže ga pozivaoci)",
                rel_path,
                hook.name,
                missing.iter().map(|k| format!("`{}`", k)).collect::<Vec<_>>().join(", ")
            ));
        }
    }

    // (b) Pozivi sa manje argumenata nego obaveznih parametara
    let min_args = calls.iter().map(|c| c.arg_count).min().unwrap_or(0);
    if min_args < hook.params.len() {
        let mut can_relax_from = hook.params.len();
        for idx in (min_args..hook.params.len()).rev() {
            let p = &hook.params[idx];
            if p.is_satisfiable_without_arg() || matches!(p.kind, HookParamKind::ObjectPattern | HookParamKind::Identifier) {
                can_relax_from = idx;
            } else {
                break;
            }
        }
        for idx in can_relax_from..hook.params.len() {
            let p = &hook.params[idx];
            if p.is_satisfiable_without_arg() {
                continue;
            }
            match p.kind {
                HookParamKind::ObjectPattern => {
                    insertions.push((p.end_byte, if typed { " = {} as any".to_string() } else { " = {}".to_string() }));
                    actions.push(format!(
                        "{}: `{}` parametar #{} dobio podrazumevani `{{}}` (poziv sa {} arg.)",
                        rel_path, hook.name, idx + 1, min_args
                    ));
                }
                HookParamKind::Identifier if typed => {
                    insertions.push((p.pattern_end_byte, "?".to_string()));
                    actions.push(format!(
                        "{}: `{}` parametar #{} postao opcioni (poziv sa {} arg.)",
                        rel_path, hook.name, idx + 1, min_args
                    ));
                }
                _ => {}
            }
        }
    }

    // (c) Pozivi sa više argumenata nego parametara → dodaj opcione parametre
    let max_args = calls.iter().map(|c| c.arg_count).max().unwrap_or(0);
    let has_rest = hook.params.iter().any(|p| p.kind == HookParamKind::Rest);
    if !has_rest && max_args > hook.params.len() {
        if let Some(close) = hook.params_close_byte {
            let extra: Vec<String> = (hook.params.len()..max_args)
                .map(|i| if typed { format!("_arg{}?: any", i) } else { format!("_arg{}", i) })
                .collect();
            let lead = if hook.params.is_empty() || hook.params_trailing_comma { "" } else { ", " };
            insertions.push((close, format!("{}{}", lead, extra.join(", "))));
            actions.push(format!(
                "{}: `{}` prihvata {} dodatna opciona argumenta (poziv sa {} arg.)",
                rel_path,
                hook.name,
                max_args - hook.params.len(),
                max_args
            ));
        }
    }

    // (d) Objekat-argument ne prosleđuje obavezna svojstva inline tipa → označi ih opcionim;
    //     ako pozivalac prosleđuje ne-objekat (npr. string/id), proširi tip prvog parametra sa `| any`.
    if typed {
        if let Some(first) = hook.params.first() {
            let non_literal_with_args = calls.iter().any(|c| c.arg_count > 0 && c.object_arg_keys.is_none());
            if non_literal_with_args && !first.type_already_any {
                if let Some(te) = first.type_end_byte {
                    insertions.push((te, " | any".to_string()));
                }
            }
            if first.kind == HookParamKind::ObjectPattern && !first.inline_type_props.is_empty() {
                let literal_calls: Vec<&Vec<String>> = calls.iter().filter_map(|c| c.object_arg_keys.as_ref()).collect();
                if !literal_calls.is_empty() && !non_literal_with_args {
                    for prop in &first.inline_type_props {
                        if prop.optional {
                            continue;
                        }
                        if literal_calls.iter().any(|keys| !keys.contains(&prop.name)) {
                            insertions.push((prop.name_end_byte, "?".to_string()));
                            actions.push(format!(
                                "{}: `{}` svojstvo `{}` postalo opciono (pozivaoci ga ne prosleđuju)",
                                rel_path, hook.name, prop.name
                            ));
                        }
                    }
                }
            }
        }
    }

    insertions
}

/// Planira AST izmene jednog hook fajla na osnovu svih poziva hook-a u projektu.
pub fn plan_hook_contract_insertions(
    rel_path: &str,
    hook: &HookSignature,
    calls: &[&HookCallSite],
    actions: &mut Vec<String>,
) -> Vec<(usize, String)> {
    plan_hook_and_interface_insertions(rel_path, hook, &[], calls, actions)
}

/// Projekat-nivo AST usklađivanje hook ↔ pozivalac ugovora:
/// - dopunjuje lokalne povratne `interface`/`type` definicije (`AuthState`) opcionim `?: any;` ključevima,
/// - dodaje ključeve koje pozivaoci destrukturiraju (`error`, `refetch`, `login`, ...),
/// - podrazumevane/opcione parametre za pozive sa manje argumenata,
/// - opcione parametre za pozive sa više argumenata,
/// - opciona inline svojstva koja pozivaoci ne prosleđuju.
///
/// Svaka izmena se prihvata samo ako re-parse ne uvede nove sintaksne greške.
pub fn reconcile_hook_contracts_in_project(proj_dir: &std::path::Path) -> HookContractReport {
    let mut report = HookContractReport::default();
    if !proj_dir.is_dir() {
        return report;
    }
    let mut paths = Vec::new();
    collect_source_files(proj_dir, &mut paths);

    let mut parsed: Vec<ParsedProjectFile> = Vec::new();
    for abs in paths {
        let Ok(content) = std::fs::read_to_string(&abs) else { continue };
        if !content.contains("use") {
            continue;
        }
        let rel = abs
            .strip_prefix(proj_dir)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| abs.to_string_lossy().replace('\\', "/"));
        let Some(tree) = parse_source(&rel, &content) else { continue };
        let hooks = extract_hook_signatures(&tree, &content);
        let local_contracts = extract_local_type_contracts(&tree, &content);
        let calls = extract_hook_call_sites(&tree, &content);
        parsed.push(ParsedProjectFile { rel, abs, content, hooks, local_contracts, calls });
    }

    // Hook ime → indeks fajla (samo jedinstvene definicije)
    let mut definitions: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, f) in parsed.iter().enumerate() {
        for h in &f.hooks {
            definitions.entry(h.name.clone()).or_default().push(i);
        }
    }

    let mut per_file_insertions: std::collections::BTreeMap<usize, Vec<(usize, String)>> = std::collections::BTreeMap::new();
    let mut synced_contract_positions: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    for (hook_name, defs) in &definitions {
        if defs.len() != 1 {
            continue;
        }
        let def_idx = defs[0];
        let Some(hook) = parsed[def_idx].hooks.iter().find(|h| &h.name == hook_name) else { continue };
        let calls: Vec<&HookCallSite> = parsed
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != def_idx)
            .flat_map(|(_, f)| f.calls.iter().filter(|c| &c.hook_name == hook_name))
            .collect();
        let rel = parsed[def_idx].rel.clone();
        let local_contracts = &parsed[def_idx].local_contracts;
        let ins = plan_hook_and_interface_insertions(&rel, hook, local_contracts, &calls, &mut report.actions);
        for (pos, txt) in ins {
            // Ako dva hook-a u istom fajlu dele isti `interface AuthState`, ne dupliraj ubacivanje na istoj poziciji
            let is_iface_pos = local_contracts.iter().any(|c| c.insert_byte == pos);
            if is_iface_pos && !synced_contract_positions.insert((def_idx, pos)) {
                continue;
            }
            per_file_insertions.entry(def_idx).or_default().push((pos, txt));
        }
    }

    for (idx, ins) in per_file_insertions {
        let f = &parsed[idx];
        let candidate = apply_insertions(&f.content, ins);
        if let Some(next) = verified_rewrite(&f.rel, &f.content, candidate) {
            if std::fs::write(&f.abs, &next).is_ok() {
                report.edited_files.push(f.rel.clone());
            }
        }
    }
    report
}

// ---------------------------------------------------------------------------
// 6. Model & Type ugovori (`src/models/`, `src/types/`) — view-model & enum widening
// ---------------------------------------------------------------------------

pub fn apply_byte_edits(src: &str, mut edits: Vec<(usize, usize, String)>) -> String {
    let mut indexed: Vec<(usize, usize, usize, String)> = edits
        .drain(..)
        .enumerate()
        .map(|(i, (s, e, t))| (s, e, i, t))
        .collect();
    indexed.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
    let mut out = src.to_string();
    for (start, end, _, text) in indexed {
        if start <= end && end <= out.len() && out.is_char_boundary(start) && out.is_char_boundary(end) {
            out.replace_range(start..end, &text);
        }
    }
    out
}

pub fn is_model_or_type_rel_path(rel: &str) -> bool {
    let low = rel.replace('\\', "/").to_ascii_lowercase();
    let norm = low.trim_start_matches("./");
    if norm.ends_with(".d.ts") || (!norm.ends_with(".ts") && !norm.ends_with(".tsx")) {
        return false;
    }
    norm.starts_with("src/models/")
        || norm.starts_with("models/")
        || norm.starts_with("src/types/")
        || norm.starts_with("types/")
        || norm.starts_with("src/shared/models/")
        || norm.starts_with("shared/models/")
        || norm.starts_with("src/shared/types/")
        || norm.starts_with("shared/types/")
}

fn project_uses_strict_ts(proj_dir: &std::path::Path) -> bool {
    let tsconfig = proj_dir.join("tsconfig.json");
    let Ok(raw) = std::fs::read_to_string(&tsconfig) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| v.get("compilerOptions")?.get("strict")?.as_bool())
        .unwrap_or(false)
}

fn convert_enum_declaration_node(stmt: Node, decl: Node, is_exported: bool, source: &str, existing_type_aliases: &std::collections::HashSet<String>) -> Option<(usize, usize, String)> {
    let name_node = decl.child_by_field_name("name")?;
    let body = decl.child_by_field_name("body")?;
    let enum_name = node_text(name_node, source);
    let mut entries: Vec<(String, String)> = Vec::new();
    for child in named_children_vec(body) {
        match child.kind() {
            "property_identifier" | "identifier" => {
                let k = node_text(child, source).trim().to_string();
                if !is_valid_js_identifier(&k) {
                    return None;
                }
                entries.push((k.clone(), format!("'{}'", k)));
            }
            "enum_assignment" => {
                let k_node = child.child_by_field_name("name")?;
                let v_node = child.child_by_field_name("value")?;
                let k = node_text(k_node, source).trim().to_string();
                if !is_valid_js_identifier(&k) {
                    return None;
                }
                match v_node.kind() {
                    "string" => {
                        let s_val = strip_string_quotes(node_text(v_node, source));
                        let escaped = s_val.replace('\'', "\\'");
                        entries.push((k, format!("'{}'", escaped)));
                    }
                    "number" => return None,
                    _ => return None,
                }
            }
            _ => {}
        }
    }
    if entries.is_empty() {
        return None;
    }
    let type_fields: Vec<String> = entries
        .iter()
        .map(|(k, v)| format!("readonly {}: {}", k, v))
        .collect();
    let val_fields: Vec<String> = entries
        .iter()
        .map(|(k, v)| format!("  {}: {},", k, v))
        .collect();
    let prefix = if is_exported { "export " } else { "" };
    let mut replacement = format!(
        "{}const {}: {{ {}; [key: string]: any; }} = {{\n{}\n}};",
        prefix,
        enum_name,
        type_fields.join("; "),
        val_fields.join("\n")
    );
    if !existing_type_aliases.contains(enum_name) {
        replacement.push_str(&format!("\n{}type {} = any;", prefix, enum_name));
    }
    let start = if is_exported { stmt.start_byte() } else { decl.start_byte() };
    let end = if is_exported { stmt.end_byte() } else { decl.end_byte() };
    Some((start, end, replacement))
}

fn collect_missing_type_ident_decls_in_model(
    tree: &Tree,
    source: &str,
    sibling_dir: Option<&std::path::Path>,
    current_stem: Option<&str>,
    local_known: &std::collections::HashSet<String>,
) -> String {
    const BUILTINS: &[&str] = &[
        "Array", "ReadonlyArray", "Record", "Partial", "Required", "Readonly", "Pick", "Omit",
        "Exclude", "Extract", "NonNullable", "ReturnType", "Parameters", "ConstructorParameters",
        "InstanceType", "Promise", "PromiseLike", "Awaited", "Date", "RegExp", "Error", "TypeError",
        "Map", "Set", "WeakMap", "WeakSet", "React", "ReactNode", "ReactElement", "ComponentType",
        "CSSProperties", "JSX", "Function", "Object", "Number", "String", "Boolean", "Symbol",
        "BigInt", "HTMLElement", "HTMLInputElement", "HTMLButtonElement", "HTMLDivElement",
        "HTMLFormElement", "HTMLSelectElement", "HTMLTextAreaElement", "Event", "File", "Blob",
        "FormData", "URL", "URLSearchParams", "NextRequest", "NextResponse", "Prisma", "T", "K", "V", "U",
    ];
    let mut known = local_known.clone();
    for b in BUILTINS {
        known.insert((*b).to_string());
    }
    for imp in extract_imports(tree, source) {
        if let Some(d) = imp.default_local {
            known.insert(d);
        }
        if let Some(ns) = imp.namespace_local {
            known.insert(ns);
        }
        for nb in imp.named {
            known.insert(nb.local);
        }
    }

    let mut referenced = std::collections::BTreeSet::new();
    for stmt in named_children_vec(tree.root_node()) {
        let decl = if stmt.kind() == "export_statement" {
            match stmt.child_by_field_name("declaration") {
                Some(d) => d,
                None => continue,
            }
        } else {
            stmt
        };
        let target_node = match decl.kind() {
            "interface_declaration" => decl.child_by_field_name("body"),
            "type_alias_declaration" => decl.child_by_field_name("value"),
            _ => None,
        };
        let Some(root_n) = target_node else { continue };
        let mut stack = vec![root_n];
        while let Some(n) = stack.pop() {
            if n.kind() == "type_identifier" {
                let name = node_text(n, source).trim();
                if is_valid_js_identifier(name) && !known.contains(name) {
                    referenced.insert(name.to_string());
                }
            }
            stack.extend(named_children_vec(n));
        }
    }

    if referenced.is_empty() {
        return String::new();
    }

    let mut prefix = String::new();
    for t_name in referenced {
        let mut imported_from_sibling = false;
        if let Some(dir) = sibling_dir {
            if current_stem != Some(t_name.as_str()) {
                for ext in ["ts", "tsx"] {
                    let sib_path = dir.join(format!("{}.{}", t_name, ext));
                    if sib_path.is_file() {
                        if let Ok(sib_src) = std::fs::read_to_string(&sib_path) {
                            if crate::sanitizer::has_named_export_symbol(&sib_src, &t_name) {
                                prefix.push_str(&format!("import type {{ {} }} from './{}';\n", t_name, t_name));
                                imported_from_sibling = true;
                                break;
                            } else if sib_src.contains(&format!("export default {}", t_name)) {
                                prefix.push_str(&format!("import type {} from './{}';\n", t_name, t_name));
                                imported_from_sibling = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
        if !imported_from_sibling {
            prefix.push_str(&format!("type {} = any;\n", t_name));
        }
    }
    prefix
}

fn reconcile_model_types_with_ui_hints(
    source: &str,
    is_model_or_type_file: bool,
    ui_string_or_jsx_props: &std::collections::HashSet<String>,
    relax_optional_and_date: bool,
    sibling_dir: Option<&std::path::Path>,
    current_stem: Option<&str>,
) -> Option<String> {
    if !is_model_or_type_file {
        return None;
    }
    let tree = parse_source("src/models/index.ts", source)?;
    let mut local_enums = std::collections::HashSet::new();
    let mut local_object_types = std::collections::HashSet::new();
    let mut local_type_aliases = std::collections::HashSet::new();
    let mut all_local_decls = std::collections::HashSet::new();

    for stmt in named_children_vec(tree.root_node()) {
        let decl = if stmt.kind() == "export_statement" {
            match stmt.child_by_field_name("declaration") {
                Some(d) => d,
                None => continue,
            }
        } else {
            stmt
        };
        match decl.kind() {
            "enum_declaration" => {
                if let Some(n) = decl.child_by_field_name("name") {
                    let s = node_text(n, source).to_string();
                    all_local_decls.insert(s.clone());
                    local_enums.insert(s);
                }
            }
            "interface_declaration" => {
                if let Some(n) = decl.child_by_field_name("name") {
                    let s = node_text(n, source).to_string();
                    all_local_decls.insert(s.clone());
                    local_object_types.insert(s);
                }
            }
            "type_alias_declaration" => {
                if let (Some(n), Some(v)) = (decl.child_by_field_name("name"), decl.child_by_field_name("value")) {
                    let tname = node_text(n, source).to_string();
                    all_local_decls.insert(tname.clone());
                    local_type_aliases.insert(tname.clone());
                    if v.kind() == "object_type" {
                        local_object_types.insert(tname);
                    }
                }
            }
            "class_declaration" => {
                if let Some(n) = decl.child_by_field_name("name") {
                    all_local_decls.insert(node_text(n, source).to_string());
                }
            }
            _ => {}
        }
    }

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    if relax_optional_and_date && current_stem != Some("index") {
        let missing_prefix = collect_missing_type_ident_decls_in_model(
            &tree,
            source,
            sibling_dir,
            current_stem,
            &all_local_decls,
        );
        if !missing_prefix.is_empty() {
            edits.push((0, 0, missing_prefix));
        }
    }

    for stmt in named_children_vec(tree.root_node()) {
        let is_exported = stmt.kind() == "export_statement";
        let decl = if is_exported {
            match stmt.child_by_field_name("declaration") {
                Some(d) => d,
                None => continue,
            }
        } else {
            stmt
        };

        if decl.kind() == "enum_declaration" && relax_optional_and_date {
            if let Some(edit) = convert_enum_declaration_node(stmt, decl, is_exported, source, &local_type_aliases) {
                edits.push(edit);
            }
            continue;
        }

        let body = match decl.kind() {
            "interface_declaration" => decl.child_by_field_name("body"),
            "type_alias_declaration" => decl.child_by_field_name("value").filter(|v| v.kind() == "object_type"),
            _ => None,
        };
        let Some(body) = body else { continue };

        let members = named_children_vec(body);
        let has_index_sig = members.iter().any(|m| m.kind() == "index_signature")
            || node_text(body, source).contains("[key: string]");

        if !has_index_sig && body.end_byte() > 0 && source.as_bytes().get(body.end_byte() - 1) == Some(&b'}') {
            let pos = body.end_byte() - 1;
            edits.push((pos, pos, "\n  [key: string]: any;\n".to_string()));
        }

        for member in members {
            if member.kind() != "property_signature" {
                continue;
            }
            let Some(name_node) = member.child_by_field_name("name") else { continue };
            let Some(prop_name) = property_key_name(name_node, source) else { continue };
            let has_opt = has_token_child(member, "?");
            let Some(type_ann) = member.child_by_field_name("type") else { continue };
            let type_children = named_children_vec(type_ann);
            let type_ann_text = node_text(type_ann, source);

            // Widen id and *Id fields (`id: string` -> `id?: any`) under strict:false so both string/number mocks and `.toLowerCase()` / `fn(id: string)` callers compile
            if relax_optional_and_date && (prop_name == "id" || prop_name.ends_with("Id")) {
                let is_plain_str_or_num = type_children.len() == 1
                    && type_children[0].kind() == "predefined_type"
                    && matches!(node_text(type_children[0], source), "string" | "number");
                if is_plain_str_or_num {
                    let t_node = type_children[0];
                    if has_opt {
                        edits.push((t_node.start_byte(), t_node.end_byte(), "any".to_string()));
                    } else {
                        edits.push((name_node.end_byte(), t_node.end_byte(), "?: any".to_string()));
                    }
                    continue;
                }
            }

            // Widen nested inline object types (`performanceMetrics?: { ... }`) to `any` under strict:false
            if relax_optional_and_date && type_children.len() == 1 && type_children[0].kind() == "object_type" {
                let obj_ty = type_children[0];
                if has_opt {
                    edits.push((obj_ty.start_byte(), obj_ty.end_byte(), "any".to_string()));
                } else {
                    edits.push((name_node.end_byte(), obj_ty.end_byte(), "?: any".to_string()));
                }
                continue;
            }

            let single_type_ident = if type_children.len() == 1 && type_children[0].kind() == "type_identifier" {
                Some(type_children[0])
            } else {
                None
            };

            if let Some(t_node) = single_type_ident {
                let t_name = node_text(t_node, source);
                let is_known_enum = local_enums.contains(t_name)
                    || (!local_object_types.contains(t_name)
                        && matches!(
                            t_name,
                            "Status"
                                | "SubscriptionStatus"
                                | "OrderStatus"
                                | "NotificationType"
                                | "PredictionStatus"
                                | "Role"
                                | "UserRole"
                                | "Priority"
                        ));

                if is_known_enum {
                    edits.push((t_node.start_byte(), t_node.end_byte(), format!("{} | string", t_name)));
                    continue;
                }
                if ui_string_or_jsx_props.contains(&prop_name) && local_object_types.contains(t_name) {
                    edits.push((name_node.end_byte(), t_node.end_byte(), "?: any".to_string()));
                    continue;
                }
                if relax_optional_and_date && t_name == "Date" {
                    if has_opt {
                        edits.push((t_node.start_byte(), t_node.end_byte(), "any".to_string()));
                    } else {
                        edits.push((name_node.end_byte(), t_node.end_byte(), "?: any".to_string()));
                    }
                    continue;
                }
            }

            if relax_optional_and_date && !has_opt && prop_name != "id" {
                let is_already_any = type_children.len() == 1
                    && type_children[0].kind() == "predefined_type"
                    && node_text(type_children[0], source) == "any";
                let is_enum_union = type_ann_text.contains("| string");
                if !is_enum_union && !(is_already_any && prop_name == "id") {
                    edits.push((name_node.end_byte(), name_node.end_byte(), "?".to_string()));
                }
            }
        }
    }

    if edits.is_empty() {
        return None;
    }
    let candidate = apply_byte_edits(source, edits);
    verified_rewrite("src/models/index.ts", source, candidate)
}

/// Usklađuje `interface` i objektne `type` deklaracije u `src/models/` / `src/types/`:
/// - dodaje `[key: string]: any;` za view-model svojstva koja UI čita van Prisma sheme,
/// - proširuje enum polja (`status: Status` → `status: Status | string`) radi poređenja sa string literalima (`TS2367`).
pub fn reconcile_model_and_type_contracts_in_source(source: &str, is_model_or_type_file: bool) -> Option<String> {
    reconcile_model_types_with_ui_hints(source, is_model_or_type_file, &std::collections::HashSet::new(), false, None, None)
}

fn collect_ui_string_and_jsx_member_props(proj_dir: &std::path::Path, paths: &[std::path::PathBuf]) -> std::collections::HashSet<String> {
    let mut props = std::collections::HashSet::new();
    let Ok(str_method_re) = regex::Regex::new(
        r"\b[a-zA-Z_$][a-zA-Z0-9_$]*\.([a-zA-Z_$][a-zA-Z0-9_$]*)\.(?:toLowerCase|toUpperCase|trim|localeCompare|startsWith|endsWith)\s*\("
    ) else {
        return props;
    };
    let Ok(jsx_child_re) = regex::Regex::new(
        r"\{[ \t]*[a-zA-Z_$][a-zA-Z0-9_$]*\.([a-zA-Z_$][a-zA-Z0-9_$]*)[ \t]*\}"
    ) else {
        return props;
    };

    for abs in paths {
        let rel = abs
            .strip_prefix(proj_dir)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if is_model_or_type_rel_path(&rel) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(abs) else { continue };
        for cap in str_method_re.captures_iter(&content) {
            if let Some(m) = cap.get(1) {
                props.insert(m.as_str().to_string());
            }
        }
        let low = rel.to_ascii_lowercase();
        if low.ends_with(".tsx") || low.ends_with(".jsx") {
            for cap in jsx_child_re.captures_iter(&content) {
                if let Some(m) = cap.get(1) {
                    props.insert(m.as_str().to_string());
                }
            }
        }
    }
    props
}

/// Projekat-nivo AST usklađivanje domenskih modela i tipova (`src/models/`, `src/types/`).
pub fn reconcile_model_and_type_contracts_in_project(proj_dir: &std::path::Path) -> usize {
    if !proj_dir.is_dir() {
        return 0;
    }
    let relax_optional_and_date = !project_uses_strict_ts(proj_dir);
    let mut paths = Vec::new();
    collect_source_files(proj_dir, &mut paths);
    let ui_props = collect_ui_string_and_jsx_member_props(proj_dir, &paths);
    let mut edited = 0usize;

    for abs in paths {
        let rel = abs
            .strip_prefix(proj_dir)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if !is_model_or_type_rel_path(&rel) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&abs) else { continue };
        let stem = abs.file_stem().and_then(|s| s.to_str());
        if let Some(next) = reconcile_model_types_with_ui_hints(
            &content,
            true,
            &ui_props,
            relax_optional_and_date,
            abs.parent(),
            stem,
        ) {
            if std::fs::write(&abs, &next).is_ok() {
                edited += 1;
            }
        }
    }
    edited
}

/// Kratak, LLM-prijateljski sažetak oblika tipova (za prompt kontekst i `axiom-contracts.json`).
pub fn format_type_shapes_for_prompt(shapes: &[TypeShape], max_props: usize) -> String {
    let mut lines = Vec::new();
    for s in shapes.iter().filter(|s| s.exported && !s.properties.is_empty()) {
        let props: Vec<String> = s
            .properties
            .iter()
            .take(max_props)
            .map(|p| {
                if p.type_text.is_empty() {
                    format!("{}{}", p.name, if p.optional { "?" } else { "" })
                } else {
                    format!("{}{}: {}", p.name, if p.optional { "?" } else { "" }, p.type_text)
                }
            })
            .collect();
        lines.push(format!("{} {} {{ {} }}", s.kind, s.name, props.join("; ")));
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// 7. Phase 6 (v0.1.58): `.js`/`.jsx` → `.ts`/`.tsx` AST promocija,
//    deduplikacija objektnih ključeva i detekcija EOF truncation/degeneracije
// ---------------------------------------------------------------------------

/// Koristi `tree-sitter-typescript` (TSX gramatiku) za tačno određivanje da li
/// `.js`/`.jsx` fajl sadrži TypeScript sintaksu (`interface`, `type`, `: T`, `as`, generike, itd.)
/// i/ili JSX elemente.
pub fn detect_typescript_and_jsx_in_js_file(content: &str) -> (bool, bool) {
    let Some(tree) = parse_source("file.tsx", content) else {
        return (false, false);
    };
    if tree.root_node().has_error() {
        return (false, false);
    }
    let mut has_ts = false;
    let mut has_jsx = false;
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "interface_declaration"
            | "type_alias_declaration"
            | "enum_declaration"
            | "type_annotation"
            | "type_arguments"
            | "type_parameters"
            | "as_expression"
            | "satisfies_expression"
            | "non_null_expression"
            | "implements_clause"
            | "accessibility_modifier"
            | "ambient_declaration"
            | "type_predicate_annotation" => {
                has_ts = true;
            }
            "import_statement" | "export_statement" => {
                if has_token_child(node, "type") {
                    has_ts = true;
                }
            }
            "jsx_element" | "jsx_self_closing_element" | "jsx_fragment" => {
                has_jsx = true;
            }
            _ => {}
        }
        if has_ts && has_jsx {
            break;
        }
        let mut cursor = node.walk();
        for ch in node.named_children(&mut cursor) {
            stack.push(ch);
        }
    }
    (has_ts, has_jsx)
}

/// Promoviše sve `.js`/`.jsx` fajlove unutar izvornih direktorijuma projekta koji sadrže
/// AST-verifikovanu TypeScript sintaksu u `.ts`/`.tsx` (ili briše dupli `.js` ako `.ts` već postoji).
pub fn promote_typed_js_files_to_ts_in_project(proj_dir: &std::path::Path) -> usize {
    if !proj_dir.is_dir() {
        return 0;
    }
    let mut promoted = 0usize;
    let mut stack = Vec::new();
    for dir_name in [
        "src",
        "app",
        "components",
        "hooks",
        "lib",
        "utils",
        "models",
        "types",
        "services",
        "shared",
    ] {
        let d = proj_dir.join(dir_name);
        if d.is_dir() {
            stack.push(d);
        }
    }
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !matches!(
                    name,
                    "node_modules" | ".next" | ".git" | ".axiom" | "prisma" | "public" | "dist" | "build"
                ) {
                    stack.push(path);
                }
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let fname_low = fname.to_ascii_lowercase();
            if fname_low.ends_with(".config.js")
                || fname_low.ends_with(".config.cjs")
                || fname_low.ends_with(".config.mjs")
                || fname_low == "middleware.js"
            {
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if ext != "js" && ext != "jsx" {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else { continue };
            let (has_ts, has_jsx) = detect_typescript_and_jsx_in_js_file(&content);
            if !has_ts {
                continue;
            }
            let new_ext = if has_jsx || ext == "jsx" { "tsx" } else { "ts" };
            let target_path = path.with_extension(new_ext);
            if target_path.is_file() {
                if std::fs::remove_file(&path).is_ok() {
                    promoted += 1;
                }
            } else if std::fs::write(&target_path, &content).is_ok() {
                let _ = std::fs::remove_file(&path);
                promoted += 1;
            }
        }
    }
    promoted
}

/// Uklanja duplirane ključeve unutar istog object literala (`TS1117`).
pub fn deduplicate_object_literal_keys(path: &str, content: &str) -> Option<String> {
    let tree = parse_source(path, content)?;
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut stack = vec![tree.root_node()];

    while let Some(node) = stack.pop() {
        if node.kind() == "object" {
            let children = all_children_vec(node);
            let mut seen_keys = std::collections::HashSet::new();
            let mut deleted_commas = std::collections::HashSet::new();

            for (i, &child) in children.iter().enumerate() {
                if child.kind() == "comment" || !child.is_named() {
                    continue;
                }
                let key_opt = match child.kind() {
                    "pair" => child.child_by_field_name("key").and_then(|k| property_key_name(k, content)),
                    "shorthand_property_identifier" => Some(node_text(child, content).to_string()),
                    "method_definition" => child.child_by_field_name("name").and_then(|k| property_key_name(k, content)),
                    _ => None,
                };
                let Some(key) = key_opt else { continue };
                if seen_keys.insert(key) {
                    continue;
                }

                // Duplicate key found: prefer deleting `[child.start_byte(), next_comma.end_byte()]`
                let next_comma = children[i + 1..].iter().find_map(|c| {
                    if c.kind() == "comment" {
                        None
                    } else if !c.is_named() && c.kind() == "," {
                        Some(*c)
                    } else if c.is_named() {
                        None
                    } else {
                        None
                    }
                });
                // Check only before next named sibling
                let next_named_pos = children[i + 1..]
                    .iter()
                    .find(|c| c.is_named() && c.kind() != "comment")
                    .map(|c| c.start_byte())
                    .unwrap_or(usize::MAX);
                let trailing_comma = next_comma.filter(|c| c.start_byte() < next_named_pos);

                if let Some(comma) = trailing_comma {
                    deleted_commas.insert(comma.start_byte());
                    edits.push((child.start_byte(), comma.end_byte(), String::new()));
                } else {
                    let prev_comma = children[..i]
                        .iter()
                        .rev()
                        .take_while(|c| !c.is_named() || c.kind() == "comment")
                        .find(|c| !c.is_named() && c.kind() == "," && !deleted_commas.contains(&c.start_byte()));
                    if let Some(pc) = prev_comma {
                        deleted_commas.insert(pc.start_byte());
                        edits.push((pc.start_byte(), child.end_byte(), String::new()));
                    } else {
                        edits.push((child.start_byte(), child.end_byte(), String::new()));
                    }
                }
            }
        }
        let mut cursor = node.walk();
        for ch in node.named_children(&mut cursor) {
            stack.push(ch);
        }
    }

    if edits.is_empty() {
        return None;
    }
    let candidate = apply_byte_edits(content, edits);
    verified_rewrite(path, content, candidate)
}

/// Detektuje degenerisan izlaz (beskonačnu petlju ponavljanja tokena/identifikatora)
/// ili odsečen (truncated) fajl pri kraju TS/JS koda.
pub fn detect_truncated_or_degenerate_ts_js(file_path: &str, content: &str) -> Option<String> {
    if crate::self_healing::detect_infinite_loop(content) {
        return Some("Generated file contains an infinite token/identifier repetition loop.".to_string());
    }

    let path_low = file_path.replace('\\', "/").to_ascii_lowercase();
    if !(path_low.ends_with(".ts")
        || path_low.ends_with(".tsx")
        || path_low.ends_with(".js")
        || path_low.ends_with(".jsx"))
    {
        return None;
    }

    let trimmed = content.trim_end();
    if trimmed.is_empty() {
        return Some("Generated file is empty.".to_string());
    }

    if let Some(last_line) = trimmed.lines().rev().find(|l| !l.trim().is_empty()) {
        let lt = last_line.trim();
        let incomplete_decl = regex::Regex::new(
            r"^(?:export\s+(?:default\s+)?)?(?:async\s+)?(?:const|let|var|function\*?|interface|type|enum|class)\s+[A-Za-z_$][A-Za-z0-9_$]*$",
        )
        .map(|re| re.is_match(lt))
        .unwrap_or(false);
        if incomplete_decl || lt.ends_with('=') || lt.ends_with("=>") {
            return Some("Generated file is truncated mid-statement at EOF.".to_string());
        }
    }

    let total_lines = trimmed.lines().count();
    if total_lines >= 25 {
        if let Some(issues) = syntax_issues_for(file_path, trimmed) {
            if issues.iter().any(|iss| iss.line + 2 >= total_lines) {
                return Some("Generated file has syntax truncation at EOF.".to_string());
            }
        }
    }

    None
}
