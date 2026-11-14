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
    HookParam {
        kind,
        start_byte: param.start_byte(),
        end_byte: param.end_byte(),
        pattern_end_byte: pattern.end_byte(),
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

fn build_hook_signature(name: &str, decl_node: Node, func: Node, src: &str) -> HookSignature {
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
    HookSignature {
        name: name.to_string(),
        line: decl_node.start_position().row + 1,
        params,
        params_close_byte,
        params_trailing_comma,
        return_objects,
        has_explicit_return_type: func.child_by_field_name("return_type").is_some(),
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
                        hooks.push(build_hook_signature(name, decl, decl, src));
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
                        hooks.push(build_hook_signature(name, decl, func, src));
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

/// Planira AST izmene jednog hook fajla na osnovu svih poziva hook-a u projektu.
pub fn plan_hook_contract_insertions(
    rel_path: &str,
    hook: &HookSignature,
    calls: &[&HookCallSite],
    actions: &mut Vec<String>,
) -> Vec<(usize, String)> {
    let typed = is_typed_path(rel_path);
    let mut insertions: Vec<(usize, String)> = Vec::new();
    if calls.is_empty() {
        return insertions;
    }

    // (a) Ključevi koje pozivaoci destrukturiraju a hook ih ne vraća
    if !hook.has_explicit_return_type && !hook.return_objects.is_empty() && hook.return_objects.iter().all(|r| !r.has_spread) {
        let mut demanded: Vec<String> = Vec::new();
        for c in calls {
            for k in &c.destructured_keys {
                if !demanded.contains(k) {
                    demanded.push(k.clone());
                }
            }
        }
        for ret in &hook.return_objects {
            let missing: Vec<&String> = demanded.iter().filter(|k| !ret.keys.contains(k)).collect();
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

    // (d) Objekat-argument ne prosleđuje obavezna svojstva inline tipa → označi ih opcionim
    if typed {
        if let Some(first) = hook.params.first() {
            if first.kind == HookParamKind::ObjectPattern && !first.inline_type_props.is_empty() {
                let literal_calls: Vec<&Vec<String>> = calls.iter().filter_map(|c| c.object_arg_keys.as_ref()).collect();
                let non_literal_with_args = calls.iter().any(|c| c.arg_count > 0 && c.object_arg_keys.is_none());
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

/// Projekat-nivo AST usklađivanje hook ↔ pozivalac ugovora:
/// - dodaje ključeve koje pozivaoci destrukturiraju (`error`, `refetch`, ...),
/// - podrazumevane/opcione parametre za pozive sa manje argumenata,
/// - opcione parametre za pozive sa više argumenata,
/// - opciona inline svojstva koja pozivaoci ne prosleđuju.
///
/// Svaka izmena je čisto ubacivanje teksta i prihvata se samo ako re-parse ne uvede nove sintaksne greške.
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
        let calls = extract_hook_call_sites(&tree, &content);
        parsed.push(ParsedProjectFile { rel, abs, content, hooks, calls });
    }

    // Hook ime → indeks fajla (samo jedinstvene definicije)
    let mut definitions: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, f) in parsed.iter().enumerate() {
        for h in &f.hooks {
            definitions.entry(h.name.clone()).or_default().push(i);
        }
    }

    let mut per_file_insertions: std::collections::BTreeMap<usize, Vec<(usize, String)>> = std::collections::BTreeMap::new();
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
        let ins = plan_hook_contract_insertions(&rel, hook, &calls, &mut report.actions);
        if !ins.is_empty() {
            per_file_insertions.entry(def_idx).or_default().extend(ins);
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
