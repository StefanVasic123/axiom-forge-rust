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
    pub local_declared_idents: Vec<String>,
    pub array_state_idents: Vec<String>,
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
    let mut local_declared_idents = Vec::new();
    let mut array_state_idents = Vec::new();
    if let Some(body) = func.child_by_field_name("body") {
        collect_return_objects(body, src, &mut return_objects);
        if body.kind() == "statement_block" {
            for stmt in named_children_vec(body) {
                match stmt.kind() {
                    "function_declaration" | "generator_function_declaration" => {
                        if let Some(nn) = stmt.child_by_field_name("name") {
                            let id = node_text(nn, src).trim().to_string();
                            if is_valid_js_identifier(&id) && !local_declared_idents.contains(&id) {
                                local_declared_idents.push(id);
                            }
                        }
                    }
                    "lexical_declaration" | "variable_declaration" => {
                        for vd in named_children_vec(stmt) {
                            if vd.kind() != "variable_declarator" {
                                continue;
                            }
                            let Some(lhs) = vd.child_by_field_name("name") else { continue };
                            let rhs = vd.child_by_field_name("value").map(unwrap_expression);
                            if lhs.kind() == "identifier" {
                                let id = node_text(lhs, src).trim().to_string();
                                if is_valid_js_identifier(&id) && !local_declared_idents.contains(&id) {
                                    local_declared_idents.push(id.clone());
                                }
                                if let Some(v) = rhs {
                                    if v.kind() == "array" && !array_state_idents.contains(&id) {
                                        array_state_idents.push(id);
                                    }
                                }
                            } else if lhs.kind() == "array_pattern" {
                                let pat_idents: Vec<String> = named_children_vec(lhs)
                                    .into_iter()
                                    .filter(|c| c.kind() == "identifier")
                                    .map(|c| node_text(c, src).trim().to_string())
                                    .filter(|id| is_valid_js_identifier(id))
                                    .collect();
                                for id in &pat_idents {
                                    if !local_declared_idents.contains(id) {
                                        local_declared_idents.push(id.clone());
                                    }
                                }
                                if let (Some(first_id), Some(v)) = (pat_idents.first(), rhs) {
                                    if v.kind() == "call_expression" {
                                        let fn_txt = v
                                            .child_by_field_name("function")
                                            .map(|n| node_text(n, src).trim())
                                            .unwrap_or("");
                                        if fn_txt == "useState" || fn_txt.ends_with(".useState") {
                                            let ta_txt = v
                                                .child_by_field_name("type_arguments")
                                                .map(|n| node_text(n, src))
                                                .unwrap_or("");
                                            let first_arg_is_arr = v
                                                .child_by_field_name("arguments")
                                                .and_then(|a| named_children_vec(a).into_iter().next())
                                                .map(|a| unwrap_expression(a).kind() == "array")
                                                .unwrap_or(false);
                                            if first_arg_is_arr
                                                || ta_txt.contains("[]")
                                                || ta_txt.contains("Array<")
                                            {
                                                if !array_state_idents.contains(first_id) {
                                                    array_state_idents.push(first_id.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
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
        local_declared_idents,
        array_state_idents,
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
    if matches!(
        low.as_str(),
        "items" | "data" | "list" | "records" | "rows" | "entries" | "results"
    ) || low.ends_with("list")
        || low.ends_with("items")
    {
        return any("[]");
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
                "node_modules" | ".next" | ".expo" | ".expo-verify" | ".expo-e2e-dist" | ".git" | ".axiom" | "dist" | "build" | "prisma" | "public" | "coverage"
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

/// Izvlači AST `type_identifier` čvorove (van lokalnih `type_parameter`/`nested_type_identifier`
/// deklaracija) koji nisu u `known` ili ugrađenim TS/React tipovima.
pub fn collect_missing_type_identifiers_ast(
    content: &str,
    known: &std::collections::HashSet<String>,
) -> Vec<String> {
    let Some(tree) = parse_source("file.tsx", content) else {
        return Vec::new();
    };
    const BUILTINS: &[&str] = &[
        "React", "FC", "FunctionComponent", "ReactNode", "ReactElement", "CSSProperties",
        "ComponentType", "PropsWithChildren", "MouseEvent", "ChangeEvent", "FormEvent",
        "KeyboardEvent", "FocusEvent", "SyntheticEvent", "RefObject", "MutableRefObject",
        "Dispatch", "SetStateAction", "Promise", "Record", "Partial", "Required", "Readonly",
        "Pick", "Omit", "Exclude", "Extract", "NonNullable", "ReturnType", "Parameters",
        "Awaited", "Array", "ReadonlyArray", "Map", "Set", "WeakMap", "WeakSet", "Date",
        "Error", "RegExp", "URL", "URLSearchParams", "Headers", "Request", "Response",
        "HTMLElement", "HTMLInputElement", "HTMLButtonElement", "HTMLDivElement",
        "HTMLFormElement", "HTMLSelectElement", "HTMLTextAreaElement", "Event", "File",
        "Blob", "FormData", "NextRequest", "NextResponse", "Metadata", "JSX", "Prisma",
        "ZodError", "ZodSchema", "ZodType", "IconType", "View", "Text", "StyleProp",
    ];
    let mut local_skip = known.clone();
    for b in BUILTINS {
        local_skip.insert((*b).to_string());
    }

    // Collect local generic type parameter names `<T, K>` and declared types/interfaces/enums/classes
    let mut type_param_names = std::collections::HashSet::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "type_parameter" => {
                for ch in named_children_vec(node) {
                    if ch.kind() == "type_identifier" {
                        type_param_names.insert(node_text(ch, content).trim().to_string());
                    }
                }
            }
            "interface_declaration" | "type_alias_declaration" | "enum_declaration" | "class_declaration" => {
                if let Some(n) = node.child_by_field_name("name") {
                    local_skip.insert(node_text(n, content).trim().to_string());
                }
            }
            _ => {}
        }
        stack.extend(named_children_vec(node));
    }

    let mut out = Vec::new();
    let mut walk_stack = vec![tree.root_node()];
    while let Some(node) = walk_stack.pop() {
        if node.kind() == "type_identifier" {
            let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");
            if !matches!(
                parent_kind,
                "type_parameter"
                    | "nested_type_identifier"
                    | "interface_declaration"
                    | "type_alias_declaration"
                    | "enum_declaration"
                    | "class_declaration"
            ) {
                let name = node_text(node, content).trim();
                if is_valid_js_identifier(name)
                    && name.chars().next().map_or(false, |c| c.is_ascii_uppercase())
                    && !local_skip.contains(name)
                    && !type_param_names.contains(name)
                    && !out.iter().any(|x| x == name)
                {
                    out.push(name.to_string());
                }
            }
        }
        walk_stack.extend(named_children_vec(node));
    }
    out
}

fn add_rest_param_insertion(
    fp: Node,
    src: &str,
    typed: bool,
    require_non_empty: bool,
    seen_positions: &mut std::collections::HashSet<usize>,
    insertions: &mut Vec<(usize, String)>,
) {
    if fp.kind() != "formal_parameters" {
        return;
    }
    let param_nodes: Vec<Node> = named_children_vec(fp)
        .into_iter()
        .filter(|c| {
            matches!(
                c.kind(),
                "required_parameter"
                    | "optional_parameter"
                    | "rest_pattern"
                    | "identifier"
                    | "object_pattern"
                    | "array_pattern"
            )
        })
        .collect();
    if require_non_empty && param_nodes.is_empty() {
        return;
    }
    let fp_text = node_text(fp, src);
    if fp_text.contains("...")
        || param_nodes
            .iter()
            .any(|p| p.kind() == "rest_pattern" || node_text(*p, src).trim_start().starts_with("..."))
    {
        return;
    }
    let tokens = all_children_vec(fp);
    if let Some(close) = tokens.iter().rev().find(|c| !c.is_named() && c.kind() == ")") {
        let pos = close.start_byte();
        if !seen_positions.insert(pos) {
            return;
        }
        let rest_suffix = if typed { "..._args: any[]" } else { "..._args" };
        let prefix = if param_nodes.is_empty() {
            ""
        } else {
            let last_end = param_nodes.last().map(|p| p.end_byte()).unwrap_or(fp.start_byte());
            let has_trailing_comma = tokens
                .iter()
                .any(|c| !c.is_named() && c.kind() == "," && c.start_byte() >= last_end && c.start_byte() < pos);
            if has_trailing_comma { " " } else { ", " }
        };
        insertions.push((pos, format!("{}{}", prefix, rest_suffix)));
    }
}

fn collect_inner_hook_fn_rest_insertions(
    body: Node,
    src: &str,
    typed: bool,
    seen_positions: &mut std::collections::HashSet<usize>,
    insertions: &mut Vec<(usize, String)>,
) {
    if body.kind() != "statement_block" {
        return;
    }
    for stmt in named_children_vec(body) {
        match stmt.kind() {
            "function_declaration" | "generator_function_declaration" => {
                if let Some(fp) = stmt.child_by_field_name("parameters") {
                    add_rest_param_insertion(fp, src, typed, true, seen_positions, insertions);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                for d in named_children_vec(stmt) {
                    if d.kind() != "variable_declarator" {
                        continue;
                    }
                    let Some(val) = d.child_by_field_name("value") else {
                        continue;
                    };
                    let unwrapped = unwrap_expression(val);
                    if matches!(unwrapped.kind(), "arrow_function" | "function_expression" | "function") {
                        if let Some(fp) = unwrapped.child_by_field_name("parameters") {
                            add_rest_param_insertion(fp, src, typed, true, seen_positions, insertions);
                        }
                    } else if unwrapped.kind() == "call_expression" {
                        let fn_name = unwrapped
                            .child_by_field_name("function")
                            .map(|f| node_text(f, src).trim())
                            .unwrap_or("");
                        if fn_name == "useCallback" || fn_name.ends_with(".useCallback") {
                            if let Some(args_node) = unwrapped.child_by_field_name("arguments") {
                                if let Some(first_arg) = named_children_vec(args_node).into_iter().next() {
                                    let cb = unwrap_expression(first_arg);
                                    if matches!(cb.kind(), "arrow_function" | "function_expression" | "function") {
                                        if let Some(fp) = cb.child_by_field_name("parameters") {
                                            add_rest_param_insertion(fp, src, typed, true, seen_positions, insertions);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Dodaje `..._args: any[]` na metode unutar `*State`/`*Return`/`*Context` interfejsa
/// i lokalne funkcije unutar `use*` hook-ova (sa >= 1 parametrom) kako višak argumenata
/// u pozivaocima ne bi izazivao `TS2554`.
pub fn collect_hook_method_rest_insertions(
    tree: &Tree,
    src: &str,
    rel_path: &str,
    hooks: &[HookSignature],
) -> Vec<(usize, String)> {
    let typed = is_typed_path(rel_path);
    let mut insertions = Vec::new();
    let mut seen_positions = std::collections::HashSet::new();
    let hook_ret_types: std::collections::HashSet<&str> = hooks
        .iter()
        .filter_map(|h| h.explicit_return_type_name.as_deref())
        .collect();

    let is_state_contract_name = |name: &str| -> bool {
        hook_ret_types.contains(name)
            || name.ends_with("State")
            || name.ends_with("Return")
            || name.ends_with("Result")
            || name.ends_with("Context")
            || name.ends_with("Actions")
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
            "interface_declaration" if typed => {
                let cname = decl
                    .child_by_field_name("name")
                    .map(|n| node_text(n, src).trim())
                    .unwrap_or("");
                if !is_state_contract_name(cname) {
                    continue;
                }
                if let Some(body) = decl.child_by_field_name("body") {
                    for member in named_children_vec(body) {
                        if member.kind() == "property_signature" {
                            if let Some(ty) = member.child_by_field_name("type") {
                                if let Some(fn_ty) = first_named_of_kind(ty, &["function_type"]) {
                                    if let Some(fp) = fn_ty.child_by_field_name("parameters") {
                                        add_rest_param_insertion(fp, src, typed, false, &mut seen_positions, &mut insertions);
                                    }
                                }
                            }
                        } else if member.kind() == "method_signature" {
                            if let Some(fp) = member.child_by_field_name("parameters") {
                                add_rest_param_insertion(fp, src, typed, false, &mut seen_positions, &mut insertions);
                            }
                        }
                    }
                }
            }
            "type_alias_declaration" if typed => {
                let cname = decl
                    .child_by_field_name("name")
                    .map(|n| node_text(n, src).trim())
                    .unwrap_or("");
                if !is_state_contract_name(cname) {
                    continue;
                }
                if let Some(val) = decl.child_by_field_name("value") {
                    if val.kind() == "object_type" {
                        for member in named_children_vec(val) {
                            if member.kind() == "property_signature" {
                                if let Some(ty) = member.child_by_field_name("type") {
                                    if let Some(fn_ty) = first_named_of_kind(ty, &["function_type"]) {
                                        if let Some(fp) = fn_ty.child_by_field_name("parameters") {
                                            add_rest_param_insertion(fp, src, typed, false, &mut seen_positions, &mut insertions);
                                        }
                                    }
                                }
                            } else if member.kind() == "method_signature" {
                                if let Some(fp) = member.child_by_field_name("parameters") {
                                    add_rest_param_insertion(fp, src, typed, false, &mut seen_positions, &mut insertions);
                                }
                            }
                        }
                    }
                }
            }
            "function_declaration" | "generator_function_declaration" => {
                if let Some(n) = decl.child_by_field_name("name") {
                    if is_hook_name(node_text(n, src)) {
                        if let Some(body) = decl.child_by_field_name("body") {
                            collect_inner_hook_fn_rest_insertions(body, src, typed, &mut seen_positions, &mut insertions);
                        }
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
                    if is_hook_name(node_text(n, src)) {
                        let func = unwrap_expression(v);
                        if matches!(func.kind(), "arrow_function" | "function_expression" | "function") {
                            if let Some(body) = func.child_by_field_name("body") {
                                collect_inner_hook_fn_rest_insertions(body, src, typed, &mut seen_positions, &mut insertions);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    insertions
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
                .map(|k| {
                    if hook.local_declared_idents.iter().any(|id| id == *k) {
                        (*k).clone()
                    } else {
                        let k_low = k.to_ascii_lowercase();
                        let is_collection_alias = matches!(
                            k_low.as_str(),
                            "items" | "data" | "list" | "records" | "rows" | "entries" | "results"
                        );
                        if is_collection_alias {
                            if let Some(arr_key) = ret
                                .keys
                                .iter()
                                .find(|rk| hook.array_state_idents.iter().any(|a| a == *rk))
                            {
                                return format!("{}: {}", k, arr_key);
                            }
                        }
                        format!("{}: {}", k, default_value_for_missing_hook_key(k, typed))
                    }
                })
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

    for (idx, f) in parsed.iter().enumerate() {
        if f.hooks.is_empty() {
            continue;
        }
        if let Some(tree) = parse_source(&f.rel, &f.content) {
            let method_rest_ins = collect_hook_method_rest_insertions(&tree, &f.content, &f.rel, &f.hooks);
            if !method_rest_ins.is_empty() {
                let entry = per_file_insertions.entry(idx).or_default();
                for (pos, txt) in method_rest_ins {
                    if !entry.iter().any(|(ep, _)| *ep == pos) {
                        entry.push((pos, txt));
                    }
                }
            }
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

        if relax_optional_and_date && decl.kind() == "type_alias_declaration" {
            if let Some(val) = decl.child_by_field_name("value") {
                let val_txt = node_text(val, source).trim();
                if (val_txt.starts_with("z.infer<") || val_txt.starts_with("zod.infer<"))
                    && !val_txt.contains("Record<string, any>")
                    && !val_txt.contains("[key: string]")
                {
                    edits.push((val.end_byte(), val.end_byte(), " & Record<string, any>".to_string()));
                    continue;
                }
            }
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
            let _type_ann_text = node_text(type_ann, source);

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
                    if relax_optional_and_date && !has_opt && prop_name != "id" {
                        edits.push((
                            name_node.end_byte(),
                            t_node.end_byte(),
                            format!("?: {} | string", t_name),
                        ));
                    } else {
                        edits.push((t_node.start_byte(), t_node.end_byte(), format!("{} | string", t_name)));
                    }
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
                if !(is_already_any && prop_name == "id") {
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
        let is_hook_file = {
            let low = rel.to_ascii_lowercase();
            (low.starts_with("src/hooks/") || low.starts_with("hooks/"))
                && (low.ends_with(".ts") || low.ends_with(".tsx"))
                && !low.ends_with("/useusertier.ts")
                && !low.ends_with("/useusertier.tsx")
                && low != "useusertier.ts"
                && low != "useusertier.tsx"
        };
        if !is_model_or_type_rel_path(&rel) && !is_hook_file {
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

// ---------------------------------------------------------------------------
// 7. TS2749 type-reference AST collection & Component destructured prop defaults
// ---------------------------------------------------------------------------

/// Collects all `type_identifier` names appearing in TypeScript type-reference positions
/// (excluding declaration names and qualified `nested_type_identifier` segments).
pub fn collect_type_reference_names(path: &str, src: &str) -> std::collections::HashSet<String> {
    let mut refs = std::collections::HashSet::new();
    if !is_typed_path(path) {
        return refs;
    }
    let Some(tree) = parse_source(path, src) else {
        return refs;
    };

    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "type_identifier" {
            let is_decl_or_qualified = node.parent().map_or(false, |parent| {
                let pk = parent.kind();
                if pk == "nested_type_identifier" || pk == "type_parameter" {
                    return true;
                }
                if matches!(
                    pk,
                    "interface_declaration"
                        | "type_alias_declaration"
                        | "enum_declaration"
                        | "class_declaration"
                        | "abstract_class_declaration"
                ) {
                    if let Some(name_node) = parent.child_by_field_name("name") {
                        return name_node.id() == node.id();
                    }
                }
                false
            });
            if !is_decl_or_qualified {
                let txt = node_text(node, src).trim();
                if !txt.is_empty() {
                    refs.insert(txt.to_string());
                }
            }
        }
        let mut cursor = node.walk();
        for ch in node.named_children(&mut cursor) {
            stack.push(ch);
        }
    }
    refs
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropTypeKind {
    Array,
    Function,
    Object,
    Primitive,
    Unknown,
}

fn unwrap_ts_type_node<'t>(mut node: Node<'t>) -> Node<'t> {
    loop {
        match node.kind() {
            "type_annotation" | "parenthesized_type" => {
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

fn classify_ts_type_node(node: Node, src: &str) -> PropTypeKind {
    let n = unwrap_ts_type_node(node);
    match n.kind() {
        "array_type" | "tuple_type" => PropTypeKind::Array,
        "generic_type" => {
            let base = n
                .child_by_field_name("name")
                .map(|c| node_text(c, src).trim())
                .unwrap_or("");
            if matches!(base, "Array" | "ReadonlyArray") {
                PropTypeKind::Array
            } else if base == "Record" {
                PropTypeKind::Object
            } else {
                PropTypeKind::Unknown
            }
        }
        "union_type" => {
            let mut non_nullish = Vec::new();
            for ch in named_children_vec(n) {
                let inner = unwrap_ts_type_node(ch);
                let txt = node_text(inner, src).trim();
                if txt == "undefined" || txt == "null" {
                    continue;
                }
                non_nullish.push(classify_ts_type_node(inner, src));
            }
            if non_nullish.is_empty() {
                PropTypeKind::Unknown
            } else if non_nullish.iter().any(|k| *k == PropTypeKind::Array) {
                PropTypeKind::Array
            } else if non_nullish.iter().any(|k| *k == PropTypeKind::Function) {
                PropTypeKind::Function
            } else if non_nullish.iter().all(|k| *k == PropTypeKind::Primitive) {
                PropTypeKind::Primitive
            } else if non_nullish.iter().any(|k| *k == PropTypeKind::Object) {
                PropTypeKind::Object
            } else {
                PropTypeKind::Unknown
            }
        }
        "function_type" => PropTypeKind::Function,
        "object_type" => PropTypeKind::Object,
        "predefined_type" | "literal_type" | "null" | "undefined" => PropTypeKind::Primitive,
        _ => PropTypeKind::Unknown,
    }
}

fn extract_object_type_prop_kinds(obj_node: Node, src: &str) -> std::collections::HashMap<String, PropTypeKind> {
    let mut map = std::collections::HashMap::new();
    for member in named_children_vec(obj_node) {
        if member.kind() != "property_signature" {
            continue;
        }
        let Some(name_node) = member.child_by_field_name("name") else { continue };
        let Some(prop_name) = property_key_name(name_node, src) else { continue };
        let kind = member
            .child_by_field_name("type")
            .map(|t| classify_ts_type_node(t, src))
            .unwrap_or(PropTypeKind::Unknown);
        map.insert(prop_name, kind);
    }
    map
}

fn ast_has_jsx(node: Node) -> bool {
    let mut stack = vec![node];
    while let Some(cur) = stack.pop() {
        if matches!(cur.kind(), "jsx_element" | "jsx_self_closing_element" | "jsx_fragment") {
            return true;
        }
        let mut cursor = cur.walk();
        for ch in cur.named_children(&mut cursor) {
            stack.push(ch);
        }
    }
    false
}

fn function_redeclares_param(fn_node: Node, target_name: &str, src: &str) -> bool {
    if let Some(fp) = fn_node.child_by_field_name("parameters") {
        let mut stack = vec![fp];
        while let Some(n) = stack.pop() {
            if matches!(
                n.kind(),
                "identifier" | "shorthand_property_identifier_pattern"
            ) && node_text(n, src).trim() == target_name
            {
                // Ensure we don't match key side of `pair_pattern` if renamed `{ target: other }`
                let is_renamed_key = n.parent().map_or(false, |p| {
                    p.kind() == "pair_pattern"
                        && p.child_by_field_name("key").map_or(false, |k| k.id() == n.id())
                });
                if !is_renamed_key {
                    return true;
                }
            }
            let mut cursor = n.walk();
            for ch in n.named_children(&mut cursor) {
                if ch.kind() == "type_annotation" {
                    continue;
                }
                stack.push(ch);
            }
        }
    } else if let Some(single) = fn_node.child_by_field_name("parameter") {
        if node_text(single, src).trim() == target_name {
            return true;
        }
    }
    false
}

fn has_optional_chain_child(node: Node, src: &str) -> bool {
    all_children_vec(node)
        .into_iter()
        .any(|c| c.kind() == "optional_chain" || (!c.is_named() && node_text(c, src).trim() == "?."))
}

fn unwrapped_ident_name<'a>(node: Node, src: &'a str) -> Option<&'a str> {
    let u = unwrap_expression(node);
    if u.kind() == "identifier" {
        Some(node_text(u, src).trim())
    } else {
        None
    }
}

#[derive(Default)]
struct DestructuredPropUsage {
    has_falsy_guard: bool,
    used_array_method: bool,
    used_length: bool,
    used_string_or_primitive_method: bool,
    used_object_prop: bool,
    used_direct_call: bool,
}

fn analyze_destructured_prop_usage(body: Node, local_name: &str, src: &str) -> DestructuredPropUsage {
    const ARRAY_METHODS: &[&str] = &[
        "map", "filter", "reduce", "reduceRight", "forEach", "slice", "flatMap", "some", "every",
        "find", "findIndex", "concat", "join", "sort", "reverse", "push", "pop",
    ];
    const PRIMITIVE_METHODS: &[&str] = &[
        "trim", "toLowerCase", "toUpperCase", "split", "startsWith", "endsWith", "replace",
        "replaceAll", "substring", "substr", "charAt", "match", "padStart", "padEnd",
        "toFixed", "toPrecision", "toExponential", "toISOString", "toLocaleDateString",
        "toLocaleTimeString", "toLocaleString", "getTime", "getFullYear",
    ];

    let mut usage = DestructuredPropUsage::default();
    let mut stack = vec![body];

    while let Some(node) = stack.pop() {
        if node.id() != body.id()
            && is_function_like(node.kind())
            && function_redeclares_param(node, local_name, src)
        {
            continue;
        }

        match node.kind() {
            "unary_expression" => {
                let op = node
                    .child_by_field_name("operator")
                    .map(|o| node_text(o, src).trim())
                    .unwrap_or("");
                if op == "!" {
                    if let Some(arg) = node.child_by_field_name("argument") {
                        if unwrapped_ident_name(arg, src) == Some(local_name) {
                            usage.has_falsy_guard = true;
                        }
                    }
                }
            }
            "binary_expression" => {
                let op = node
                    .child_by_field_name("operator")
                    .map(|o| node_text(o, src).trim())
                    .unwrap_or("");
                if let (Some(left), Some(right)) = (
                    node.child_by_field_name("left"),
                    node.child_by_field_name("right"),
                ) {
                    if op == "&&" && unwrapped_ident_name(left, src) == Some(local_name) {
                        usage.has_falsy_guard = true;
                    } else if matches!(op, "==" | "===" | "!=" | "!==") {
                        let l_txt = node_text(unwrap_expression(left), src).trim();
                        let r_txt = node_text(unwrap_expression(right), src).trim();
                        if (l_txt == local_name && matches!(r_txt, "null" | "undefined"))
                            || (r_txt == local_name && matches!(l_txt, "null" | "undefined"))
                        {
                            usage.has_falsy_guard = true;
                        }
                    }
                }
            }
            "if_statement" | "ternary_expression" => {
                if let Some(cond) = node.child_by_field_name("condition") {
                    if unwrapped_ident_name(cond, src) == Some(local_name) {
                        usage.has_falsy_guard = true;
                    }
                }
            }
            "member_expression" => {
                if !has_optional_chain_child(node, src) {
                    if let Some(obj) = node.child_by_field_name("object") {
                        if unwrapped_ident_name(obj, src) == Some(local_name) {
                            if let Some(prop) = node.child_by_field_name("property") {
                                let prop_name = node_text(prop, src).trim();
                                if ARRAY_METHODS.contains(&prop_name) {
                                    usage.used_array_method = true;
                                } else if prop_name == "length" {
                                    usage.used_length = true;
                                } else if PRIMITIVE_METHODS.contains(&prop_name) {
                                    usage.used_string_or_primitive_method = true;
                                } else if !prop_name.is_empty() {
                                    usage.used_object_prop = true;
                                }
                            }
                        }
                    }
                }
            }
            "call_expression" => {
                if !has_optional_chain_child(node, src) {
                    if let Some(func_n) = node.child_by_field_name("function") {
                        if unwrapped_ident_name(func_n, src) == Some(local_name) {
                            usage.used_direct_call = true;
                        }
                    }
                }
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for ch in node.named_children(&mut cursor) {
            stack.push(ch);
        }
    }

    usage
}

fn resolve_component_prop_types<'a>(
    first_param: Node,
    var_declarator: Option<Node>,
    local_type_maps: &'a std::collections::HashMap<String, std::collections::HashMap<String, PropTypeKind>>,
    src: &str,
) -> std::collections::HashMap<String, PropTypeKind> {
    let from_type_node = |tn: Node| -> Option<std::collections::HashMap<String, PropTypeKind>> {
        let u = unwrap_ts_type_node(tn);
        if u.kind() == "object_type" {
            return Some(extract_object_type_prop_kinds(u, src));
        }
        if u.kind() == "type_identifier" {
            let name = node_text(u, src).trim();
            if let Some(found) = local_type_maps.get(name) {
                return Some(found.clone());
            }
        }
        None
    };

    if let Some(ta) = first_param.child_by_field_name("type") {
        if let Some(map) = from_type_node(ta) {
            return map;
        }
    }

    if let Some(vd) = var_declarator {
        if let Some(ta) = vd.child_by_field_name("type") {
            let mut stack = vec![ta];
            while let Some(n) = stack.pop() {
                if n.kind() == "generic_type" {
                    if let Some(args) = n.child_by_field_name("type_arguments") {
                        if let Some(first_arg) = named_children_vec(args).into_iter().next() {
                            if let Some(map) = from_type_node(first_arg) {
                                return map;
                            }
                        }
                    }
                }
                let mut cursor = n.walk();
                for ch in n.named_children(&mut cursor) {
                    stack.push(ch);
                }
            }
        }
    }

    std::collections::HashMap::new()
}

/// Automatically infers safe runtime defaults (`= []`, `= () => {}`, `= {} as any`)
/// for destructured props of React components in `.tsx`/`.jsx` files purely from TypeScript
/// prop types and unguarded AST usage in the component body — zero hardcoded prop names.
pub fn ensure_component_destructured_prop_defaults_in_source(rel_path: &str, src: &str) -> Option<String> {
    let low = rel_path.replace('\\', "/").to_ascii_lowercase();
    if !(low.ends_with(".tsx") || low.ends_with(".jsx")) {
        return None;
    }
    let tree = parse_source(rel_path, src)?;

    // Step A: Collect top-level interface/type object shapes
    let mut local_type_maps: std::collections::HashMap<String, std::collections::HashMap<String, PropTypeKind>> =
        std::collections::HashMap::new();

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
                if let (Some(name_n), Some(body_n)) = (
                    decl.child_by_field_name("name"),
                    decl.child_by_field_name("body"),
                ) {
                    let name = node_text(name_n, src).trim().to_string();
                    local_type_maps.insert(name, extract_object_type_prop_kinds(body_n, src));
                }
            }
            "type_alias_declaration" => {
                if let (Some(name_n), Some(val_n)) = (
                    decl.child_by_field_name("name"),
                    decl.child_by_field_name("value"),
                ) {
                    let u = unwrap_ts_type_node(val_n);
                    if u.kind() == "object_type" {
                        let name = node_text(name_n, src).trim().to_string();
                        local_type_maps.insert(name, extract_object_type_prop_kinds(u, src));
                    }
                }
            }
            _ => {}
        }
    }

    // Step B: Collect top-level React component functions `(func_node, var_declarator_opt)`
    let mut components: Vec<(Node, Option<Node>)> = Vec::new();
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
            "function_declaration" => {
                if let Some(name_n) = decl.child_by_field_name("name") {
                    let name = node_text(name_n, src).trim();
                    if name.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                        components.push((decl, None));
                    }
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                for vd in named_children_vec(decl) {
                    if vd.kind() != "variable_declarator" {
                        continue;
                    }
                    let (Some(name_n), Some(val_n)) = (
                        vd.child_by_field_name("name"),
                        vd.child_by_field_name("value"),
                    ) else {
                        continue;
                    };
                    let name = node_text(name_n, src).trim();
                    if !name.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                        continue;
                    }
                    let func = unwrap_expression(val_n);
                    if matches!(func.kind(), "arrow_function" | "function_expression" | "function") {
                        components.push((func, Some(vd)));
                    }
                }
            }
            _ => {}
        }
    }

    let typed = is_typed_path(rel_path);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();

    for (func_node, vd_opt) in components {
        let Some(body_node) = func_node.child_by_field_name("body") else { continue };
        if !ast_has_jsx(body_node) {
            continue;
        }
        let Some(fp_node) = func_node.child_by_field_name("parameters") else { continue };
        let Some(first_param) = named_children_vec(fp_node)
            .into_iter()
            .find(|p| matches!(p.kind(), "required_parameter" | "optional_parameter" | "object_pattern"))
        else {
            continue;
        };

        let obj_pattern = if first_param.kind() == "object_pattern" {
            first_param
        } else {
            let pat = first_param
                .child_by_field_name("pattern")
                .or_else(|| named_children_vec(first_param).into_iter().next());
            let Some(p) = pat.filter(|n| n.kind() == "object_pattern") else {
                continue;
            };
            p
        };

        let prop_types = resolve_component_prop_types(first_param, vd_opt, &local_type_maps, src);

        // Step C & D: Inspect every destructured member in `object_pattern`
        for member in named_children_vec(obj_pattern) {
            let (prop_key, local_name, insert_byte) = match member.kind() {
                "shorthand_property_identifier_pattern" => {
                    let sym = node_text(member, src).trim().to_string();
                    (sym.clone(), sym, member.end_byte())
                }
                "pair_pattern" => {
                    let (Some(k_n), Some(v_n)) = (
                        member.child_by_field_name("key"),
                        member.child_by_field_name("value"),
                    ) else {
                        continue;
                    };
                    if v_n.kind() != "identifier" {
                        // Already defaulted (`assignment_pattern`) or nested destructure
                        continue;
                    }
                    let Some(k_str) = property_key_name(k_n, src) else { continue };
                    let l_str = node_text(v_n, src).trim().to_string();
                    (k_str, l_str, v_n.end_byte())
                }
                // `object_assignment_pattern` (already defaulted), `assignment_pattern`, `rest_pattern`
                _ => continue,
            };

            if local_name.is_empty() || local_name == "children" {
                continue;
            }

            let usage = analyze_destructured_prop_usage(body_node, &local_name, src);
            if usage.has_falsy_guard {
                continue;
            }

            let decl_kind = prop_types.get(&prop_key).copied().unwrap_or(PropTypeKind::Unknown);
            let default_str = if decl_kind == PropTypeKind::Array
                || usage.used_array_method
                || (usage.used_length
                    && decl_kind != PropTypeKind::Primitive
                    && !usage.used_string_or_primitive_method)
            {
                Some(" = []")
            } else if usage.used_direct_call && decl_kind != PropTypeKind::Primitive {
                Some(if typed { " = ((() => {}) as any)" } else { " = () => {}" })
            } else if usage.used_object_prop
                && decl_kind != PropTypeKind::Primitive
                && !usage.used_string_or_primitive_method
                && !usage.used_length
            {
                Some(if typed { " = {} as any" } else { " = {}" })
            } else {
                None
            };

            if let Some(def) = default_str {
                edits.push((insert_byte, insert_byte, def.to_string()));
            }
        }
    }

    if edits.is_empty() {
        return None;
    }

    let candidate = apply_byte_edits(src, edits);
    verified_rewrite(rel_path, src, candidate)
}

/// Applies `ensure_component_destructured_prop_defaults_in_source` across all `.tsx`/`.jsx` files
/// in `proj_dir`, returning the number of updated files.
pub fn ensure_component_destructured_prop_defaults_in_project(proj_dir: &std::path::Path) -> usize {
    if !proj_dir.is_dir() {
        return 0;
    }
    let mut updated = 0usize;
    let mut stack = vec![proj_dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if path.is_dir() {
                if matches!(
                    name_str.as_ref(),
                    "node_modules" | ".next" | ".git" | "dist" | "build" | ".axiom" | "target"
                ) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let low = name_str.to_ascii_lowercase();
            if !(low.ends_with(".tsx") || low.ends_with(".jsx")) {
                continue;
            }
            let rel = path
                .strip_prefix(proj_dir)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| name_str.to_string());
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Some(fixed) = ensure_component_destructured_prop_defaults_in_source(&rel, &content) {
                    if std::fs::write(&path, fixed).is_ok() {
                        updated += 1;
                    }
                }
            }
        }
    }
    updated
}

/// Replaces non-exported local duplicate `interface X` declarations in project `.ts`/`.tsx` files
/// with `import type { X } from '@/types...'` when `@/types` already exports `X` with `[key: string]: any;`.
pub fn deduplicate_local_domain_interfaces_against_types_in_project(proj_dir: &std::path::Path) -> usize {
    use std::collections::{BTreeMap, HashMap, HashSet};

    if !proj_dir.is_dir() {
        return 0;
    }

    let (types_dir, types_alias_prefix) = if proj_dir.join("src/types").is_dir() {
        (proj_dir.join("src/types"), "@/types")
    } else if proj_dir.join("types").is_dir() && !proj_dir.join("src").is_dir() {
        (proj_dir.join("types"), "@/types")
    } else {
        return 0;
    };

    // Step A: Scan `@/types` modules and record safe exported domain types (`[key: string]: any;`)
    let mut module_safe_types: HashMap<String, HashSet<String>> = HashMap::new();
    let mut index_tree_and_src: Option<(String, Tree)> = None;

    let mut type_stack = vec![types_dir.clone()];
    while let Some(cur) = type_stack.pop() {
        let Ok(entries) = std::fs::read_dir(&cur) else { continue };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                type_stack.push(p);
                continue;
            }
            if !p.is_file() {
                continue;
            }
            let Ok(sub_rel) = p.strip_prefix(&types_dir) else { continue };
            let sub_str = sub_rel.to_string_lossy().replace('\\', "/");
            let sub_low = sub_str.to_ascii_lowercase();
            if sub_low.ends_with(".d.ts") || (!sub_low.ends_with(".ts") && !sub_low.ends_with(".tsx")) {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&p) else { continue };
            let rel_for_parse = p
                .strip_prefix(proj_dir)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| sub_str.clone());
            let Some(tree) = parse_source(&rel_for_parse, &src) else { continue };

            let contracts = extract_local_type_contracts(&tree, &src);
            let exports = extract_exports(&tree, &src);

            let mut directly_exported_names: HashSet<String> = exports.named.iter().cloned().collect();
            for stmt in named_children_vec(tree.root_node()) {
                if stmt.kind() == "export_statement" && !has_token_child(stmt, "default") {
                    if let Some(decl) = stmt.child_by_field_name("declaration") {
                        for n in declaration_names(decl, &src) {
                            directly_exported_names.insert(n);
                        }
                    }
                }
            }

            let mut safe_names = HashSet::new();
            for c in contracts {
                if c.has_index_sig && directly_exported_names.contains(&c.name) {
                    safe_names.insert(c.name);
                }
            }

            let stem = sub_str
                .trim_end_matches(".tsx")
                .trim_end_matches(".ts")
                .to_string();
            module_safe_types.insert(stem.clone(), safe_names);
            if stem == "index" {
                index_tree_and_src = Some((src, tree));
            }
        }
    }

    let mut exported_types: HashMap<String, String> = HashMap::new();
    // 1. Direct index types & index re-exports map to `@/types`
    if let Some(index_safe) = module_safe_types.get("index") {
        for name in index_safe {
            exported_types.insert(name.clone(), types_alias_prefix.to_string());
        }
    }
    if let Some((ref idx_src, ref idx_tree)) = index_tree_and_src {
        let idx_exports = extract_exports(idx_tree, idx_src);
        for star_spec in &idx_exports.star_reexports {
            let clean_spec = star_spec.trim_start_matches("./").trim_end_matches(".ts").trim_end_matches(".tsx");
            if let Some(mod_safe) = module_safe_types.get(clean_spec) {
                for name in mod_safe {
                    exported_types.insert(name.clone(), types_alias_prefix.to_string());
                }
            }
        }
        for stmt in named_children_vec(idx_tree.root_node()) {
            if stmt.kind() != "export_statement" {
                continue;
            }
            let Some(source_node) = stmt.child_by_field_name("source") else { continue };
            let src_spec = strip_string_quotes(node_text(source_node, idx_src));
            let clean_spec = src_spec.trim_start_matches("./").trim_end_matches(".ts").trim_end_matches(".tsx");
            let Some(mod_safe) = module_safe_types.get(clean_spec) else { continue };
            if let Some(clause) = first_named_of_kind(stmt, &["export_clause"]) {
                for spec in named_children_vec(clause) {
                    if spec.kind() != "export_specifier" {
                        continue;
                    }
                    let orig = spec
                        .child_by_field_name("name")
                        .map(|n| strip_string_quotes(node_text(n, idx_src)))
                        .unwrap_or_default();
                    let alias = spec
                        .child_by_field_name("alias")
                        .map(|n| strip_string_quotes(node_text(n, idx_src)))
                        .unwrap_or_else(|| orig.clone());
                    if mod_safe.contains(&orig) && !alias.is_empty() {
                        exported_types.insert(alias, types_alias_prefix.to_string());
                    }
                }
            }
        }
    }
    // 2. Any safe exported type in submodules not re-exported by `index.ts` maps to `@/types/<stem>`
    let mut sorted_stems: Vec<String> = module_safe_types.keys().cloned().collect();
    sorted_stems.sort();
    for stem in sorted_stems {
        if stem == "index" {
            continue;
        }
        if let Some(names) = module_safe_types.get(&stem) {
            let mut sorted_names: Vec<String> = names.iter().cloned().collect();
            sorted_names.sort();
            for name in sorted_names {
                exported_types
                    .entry(name)
                    .or_insert_with(|| format!("{}/{}", types_alias_prefix, stem));
            }
        }
    }

    if exported_types.is_empty() {
        return 0;
    }

    // Step B: Walk project `.ts`/`.tsx` files outside types/models and replace local duplicate interfaces
    let mut updated = 0usize;
    let mut file_stack = vec![proj_dir.to_path_buf()];
    while let Some(current) = file_stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name_os = entry.file_name();
            let name_str = name_os.to_string_lossy();
            if path.is_dir() {
                if matches!(
                    name_str.as_ref(),
                    "node_modules" | ".next" | ".git" | "dist" | "build" | "out" | "target" | ".axiom"
                ) {
                    continue;
                }
                file_stack.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let Ok(rel_pb) = path.strip_prefix(proj_dir) else { continue };
            let rel_path = rel_pb.to_string_lossy().replace('\\', "/");
            let rel_low = rel_path.to_ascii_lowercase();
            if rel_low.ends_with(".d.ts") || (!rel_low.ends_with(".ts") && !rel_low.ends_with(".tsx")) {
                continue;
            }
            if rel_low.starts_with("src/types/")
                || rel_low.starts_with("types/")
                || rel_low.starts_with("src/models/")
                || rel_low.starts_with("models/")
            {
                continue;
            }

            let Ok(content) = std::fs::read_to_string(&path) else { continue };
            let Some(tree) = parse_source(&rel_path, &content) else { continue };

            let mut imported_locals = HashSet::new();
            for imp in extract_imports(&tree, &content) {
                if let Some(d) = imp.default_local {
                    imported_locals.insert(d);
                }
                if let Some(ns) = imp.namespace_local {
                    imported_locals.insert(ns);
                }
                for b in imp.named {
                    imported_locals.insert(b.local);
                }
            }

            let mut local_value_names = HashSet::new();
            for stmt in named_children_vec(tree.root_node()) {
                let inner = if stmt.kind() == "export_statement" {
                    match stmt.child_by_field_name("declaration") {
                        Some(d) => d,
                        None => continue,
                    }
                } else {
                    stmt
                };
                if matches!(
                    inner.kind(),
                    "lexical_declaration"
                        | "variable_declaration"
                        | "function_declaration"
                        | "generator_function_declaration"
                        | "class_declaration"
                        | "abstract_class_declaration"
                        | "enum_declaration"
                ) {
                    for n in declaration_names(inner, &content) {
                        local_value_names.insert(n);
                    }
                }
            }

            let mut reexported_clause_names: HashSet<String> = HashSet::new();
            for stmt in named_children_vec(tree.root_node()) {
                if stmt.kind() == "export_statement"
                    && stmt.child_by_field_name("declaration").is_none()
                    && stmt.child_by_field_name("source").is_none()
                {
                    if let Some(clause) = first_named_of_kind(stmt, &["export_clause"]) {
                        for spec in named_children_vec(clause) {
                            if spec.kind() != "export_specifier" {
                                continue;
                            }
                            let exported = spec
                                .child_by_field_name("alias")
                                .or_else(|| spec.child_by_field_name("name"))
                                .or_else(|| named_children_vec(spec).into_iter().next())
                                .map(|n| strip_string_quotes(node_text(n, &content)))
                                .unwrap_or_default();
                            if !exported.is_empty() {
                                reexported_clause_names.insert(exported);
                            }
                        }
                    }
                }
            }

            let mut edits: Vec<(usize, usize, String)> = Vec::new();
            let mut to_import: BTreeMap<String, Vec<String>> = BTreeMap::new();

            for stmt in named_children_vec(tree.root_node()) {
                let (is_exported, iface_decl) = if stmt.kind() == "interface_declaration" {
                    (false, stmt)
                } else if stmt.kind() == "export_statement" && !has_token_child(stmt, "default") {
                    match stmt.child_by_field_name("declaration") {
                        Some(d) if d.kind() == "interface_declaration" => (true, d),
                        _ => continue,
                    }
                } else {
                    continue;
                };
                let Some(iface_name_node) = iface_decl.child_by_field_name("name") else { continue };
                let iface_name = node_text(iface_name_node, &content).trim().to_string();
                if iface_name.is_empty()
                    || iface_name.ends_with("Props")
                    || iface_name.ends_with("State")
                    || iface_name.ends_with("Options")
                    || iface_name.ends_with("Params")
                    || iface_name.ends_with("Config")
                    || iface_name.ends_with("Context")
                {
                    continue;
                }
                if imported_locals.contains(&iface_name) || local_value_names.contains(&iface_name) {
                    continue;
                }
                let Some(spec) = exported_types.get(&iface_name) else {
                    continue;
                };

                let mut del_end = stmt.end_byte();
                while del_end < content.len() && matches!(content.as_bytes()[del_end], b' ' | b'\t') {
                    del_end += 1;
                }
                if content[del_end..].starts_with("\r\n") {
                    del_end += 2;
                } else if content[del_end..].starts_with('\n') {
                    del_end += 1;
                }
                let replacement = if is_exported && !reexported_clause_names.contains(&iface_name) {
                    format!("export type {{ {} }};\n", iface_name)
                } else {
                    String::new()
                };
                edits.push((stmt.start_byte(), del_end, replacement));

                let sym_list = to_import.entry(spec.clone()).or_default();
                if !sym_list.contains(&iface_name) {
                    sym_list.push(iface_name);
                }
            }

            if edits.is_empty() {
                continue;
            }

            // Compute safe insertion byte after `'use client'` / `'use server'` and all top-level imports
            let mut insert_byte = 0usize;
            for child in named_children_vec(tree.root_node()) {
                let kind = child.kind();
                if kind == "import_statement" {
                    insert_byte = child.end_byte();
                } else if kind == "expression_statement" {
                    let txt = node_text(child, &content).trim();
                    if txt.contains("use client") || txt.contains("use server") {
                        insert_byte = child.end_byte();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            if insert_byte > 0 && content[insert_byte..].starts_with("\r\n") {
                insert_byte += 2;
            } else if insert_byte > 0 && content[insert_byte..].starts_with('\n') {
                insert_byte += 1;
            }

            let mut import_block = String::new();
            for (spec, syms) in to_import {
                import_block.push_str(&format!("import type {{ {} }} from '{}';\n", syms.join(", "), spec));
            }
            let formatted_block = if insert_byte > 0 && !content[..insert_byte].ends_with('\n') {
                format!("\n{}", import_block)
            } else {
                import_block
            };
            edits.push((insert_byte, insert_byte, formatted_block));

            let candidate = apply_byte_edits(&content, edits);
            if let Some(verified) = verified_rewrite(&rel_path, &content, candidate) {
                if std::fs::write(&path, verified).is_ok() {
                    updated += 1;
                }
            }
        }
    }

    updated
}

/// Deterministically removes nonexistent named import specifiers (e.g. from `TS2305`)
/// via AST when the imported local binding is completely unused outside `import` declarations
/// (for instance when the word only appears in comments or JSX text).
pub fn remove_unused_named_import_specifiers(
    path: &str,
    src: &str,
    targets: &[(Option<String>, String)],
) -> Option<String> {
    if targets.is_empty() {
        return None;
    }
    let tree = parse_source(path, src)?;

    let mut used_idents: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "import_statement" {
            continue;
        }
        if matches!(
            node.kind(),
            "identifier"
                | "type_identifier"
                | "jsx_identifier"
                | "shorthand_property_identifier"
                | "shorthand_property_identifier_pattern"
        ) {
            let txt = node_text(node, src).trim();
            if !txt.is_empty() {
                used_idents.insert(txt.to_string());
            }
        }
        let mut cursor = node.walk();
        for ch in node.named_children(&mut cursor) {
            stack.push(ch);
        }
    }

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for stmt in named_children_vec(tree.root_node()) {
        if stmt.kind() != "import_statement" {
            continue;
        }
        let Some(source) = stmt
            .child_by_field_name("source")
            .or_else(|| first_named_of_kind(stmt, &["string"]))
        else {
            continue;
        };
        let module_spec = strip_string_quotes(node_text(source, src));
        let Some(clause) = first_named_of_kind(stmt, &["import_clause"]) else {
            continue;
        };

        let mut has_default_or_namespace = false;
        let mut named_imports_node: Option<Node> = None;
        for part in named_children_vec(clause) {
            match part.kind() {
                "identifier" | "namespace_import" => has_default_or_namespace = true,
                "named_imports" => named_imports_node = Some(part),
                _ => {}
            }
        }
        let Some(named_imports) = named_imports_node else {
            continue;
        };

        let mut kept_raw: Vec<String> = Vec::new();
        let mut removed_any = false;
        let mut spec_count = 0usize;

        for spec in named_children_vec(named_imports) {
            if spec.kind() != "import_specifier" {
                continue;
            }
            spec_count += 1;
            let name_node = spec
                .child_by_field_name("name")
                .or_else(|| named_children_vec(spec).into_iter().next());
            let alias_node = spec.child_by_field_name("alias");
            let imported_name = name_node
                .map(|n| strip_string_quotes(node_text(n, src)))
                .unwrap_or_default();
            let local_name = alias_node
                .map(|a| node_text(a, src).trim().to_string())
                .unwrap_or_else(|| imported_name.clone());

            let matches_target = !imported_name.is_empty()
                && targets.iter().any(|(mod_opt, sym)| {
                    sym == &imported_name && mod_opt.as_ref().map_or(true, |m| m == &module_spec)
                });
            let is_unused = !local_name.is_empty() && !used_idents.contains(&local_name);

            if matches_target && is_unused {
                removed_any = true;
            } else {
                kept_raw.push(node_text(spec, src).trim().to_string());
            }
        }

        if !removed_any || spec_count == 0 {
            continue;
        }

        if kept_raw.is_empty() {
            if !has_default_or_namespace {
                let mut del_end = stmt.end_byte();
                while del_end < src.len() && matches!(src.as_bytes()[del_end], b' ' | b'\t') {
                    del_end += 1;
                }
                if src[del_end..].starts_with("\r\n") {
                    del_end += 2;
                } else if src[del_end..].starts_with('\n') {
                    del_end += 1;
                }
                edits.push((stmt.start_byte(), del_end, String::new()));
            } else {
                let prefix = &src[clause.start_byte()..named_imports.start_byte()];
                let trimmed_prefix = prefix.trim_end();
                let del_start = if let Some(comma_prefix) = trimmed_prefix.strip_suffix(',') {
                    clause.start_byte() + comma_prefix.len()
                } else {
                    named_imports.start_byte()
                };
                edits.push((del_start, named_imports.end_byte(), String::new()));
            }
        } else {
            edits.push((
                named_imports.start_byte(),
                named_imports.end_byte(),
                format!("{{ {} }}", kept_raw.join(", ")),
            ));
        }
    }

    if edits.is_empty() {
        return None;
    }

    let candidate = apply_byte_edits(src, edits);
    verified_rewrite(path, src, candidate)
}

/// Relaxes local non-Props domain view-model interfaces in UI components (e.g. `interface AssemblyNode { id: string; ... }`)
/// in non-strict TS projects by adding `[key: string]: any;` and making fields optional, preventing TS2322/TS2339 when
/// spreading widened `@/types` records into local tree/card/table view-models.
pub fn widen_local_component_view_models_in_source(rel_path: &str, src: &str) -> Option<String> {
    let low = rel_path.replace('\\', "/").to_ascii_lowercase();
    if !low.ends_with(".tsx") && !low.ends_with(".ts") {
        return None;
    }
    if low.ends_with(".d.ts") || low.contains("/api/") || low.starts_with("api/") {
        return None;
    }
    let tree = parse_source(rel_path, src)?;
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut touched: Vec<(usize, usize)> = Vec::new();

    for stmt in named_children_vec(tree.root_node()) {
        let decl = if stmt.kind() == "export_statement" {
            match stmt.child_by_field_name("declaration") {
                Some(d) => d,
                None => continue,
            }
        } else {
            stmt
        };
        if decl.kind() != "interface_declaration" {
            continue;
        }
        let Some(name_n) = decl.child_by_field_name("name") else { continue };
        let iname = node_text(name_n, src).trim();
        let iname_low = iname.to_ascii_lowercase();
        if iname_low.ends_with("props")
            || iname_low.ends_with("options")
            || iname_low.ends_with("state")
            || iname_low.ends_with("context")
            || iname_low.ends_with("config")
            || iname_low.ends_with("params")
            || iname_low.ends_with("result")
        {
            continue;
        }
        let Some(body) = decl.child_by_field_name("body") else { continue };
        let body_txt = node_text(body, src);
        if body_txt.contains("[key:") {
            continue;
        }
        // Only treat as a domain entity view-model if it declares an `id` property
        let mut has_id_prop = false;
        let mut required_prop_name_ends: Vec<usize> = Vec::new();
        for member in named_children_vec(body) {
            if member.kind() == "property_signature" {
                let Some(pn) = member.child_by_field_name("name") else { continue };
                let pname = strip_string_quotes(node_text(pn, src));
                if pname == "id" {
                    has_id_prop = true;
                }
                let member_txt = node_text(member, src);
                let after_name = &src[pn.end_byte()..member.end_byte()];
                if !after_name.trim_start().starts_with('?') && !member_txt.contains('?') {
                    required_prop_name_ends.push(pn.end_byte());
                }
            }
        }
        if !has_id_prop {
            continue;
        }
        for pos in required_prop_name_ends {
            if !touched.iter().any(|&(s, e)| pos >= s && pos <= e) {
                edits.push((pos, pos, "?".to_string()));
                touched.push((pos, pos));
            }
        }
        let close_brace = body.end_byte().saturating_sub(1);
        if close_brace > body.start_byte() && src.as_bytes()[close_brace] == b'}' {
            edits.push((close_brace, close_brace, "  [key: string]: any;\n".to_string()));
        }
    }

    if edits.is_empty() {
        return None;
    }
    let candidate = apply_byte_edits(src, edits);
    verified_rewrite(rel_path, src, candidate)
}

pub fn widen_local_component_view_models_in_project(proj_dir: &std::path::Path) -> usize {
    if project_uses_strict_ts(proj_dir) {
        return 0;
    }
    let mut updated = 0usize;
    let mut files = Vec::new();
    for root in ["src/components", "src/renderer", "src/app", "components", "app"] {
        let dir = proj_dir.join(root);
        if dir.is_dir() {
            collect_source_files(&dir, &mut files);
        }
    }
    for abs_path in files {
        let rel_path = abs_path
            .strip_prefix(proj_dir)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let Ok(orig) = std::fs::read_to_string(&abs_path) else { continue };
        if let Some(next) = widen_local_component_view_models_in_source(&rel_path, &orig) {
            if next != orig && std::fs::write(&abs_path, next).is_ok() {
                updated += 1;
            }
        }
    }
    updated
}

/// Universally guards custom hook (`useXxx`) array parameters and `useState<...[]>(param)` calls
/// against runtime `undefined.map()` / `undefined.length` crashes when callers invoke the hook
/// with fewer arguments or a mismatched argument type.
pub fn ensure_hook_array_param_defaults_in_source(rel_path: &str, src: &str) -> Option<String> {
    let lower = rel_path.replace('\\', "/").to_ascii_lowercase();
    let is_ts = lower.ends_with(".ts") || lower.ends_with(".tsx");
    let is_js = lower.ends_with(".js") || lower.ends_with(".jsx");
    if !is_ts && !is_js {
        return None;
    }
    if lower.contains("/api/") || lower.starts_with("api/") {
        return None;
    }
    let tree = parse_source(rel_path, src)?;
    let root = tree.root_node();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut touched_ranges: Vec<(usize, usize)> = Vec::new();

    fn overlaps(ranges: &[(usize, usize)], s: usize, e: usize) -> bool {
        ranges.iter().any(|&(rs, re)| s < re && rs < e)
    }

    fn is_array_ts_type(node: Node, src: &str) -> bool {
        let txt = node_text(node, src).trim();
        if txt.contains("[]") || txt.starts_with("Array<") || txt.starts_with("ReadonlyArray<") {
            return true;
        }
        false
    }

    fn find_hook_functions<'a>(node: Node<'a>, src: &str, out: &mut Vec<(Node<'a>, Node<'a>)>) {
        match node.kind() {
            "function_declaration" => {
                if let Some(name_n) = node.child_by_field_name("name") {
                    let name = node_text(name_n, src).trim();
                    if name.starts_with("use") && name.chars().nth(3).map_or(false, |c| c.is_ascii_uppercase()) {
                        if let (Some(fp), Some(body)) = (node.child_by_field_name("parameters"), node.child_by_field_name("body")) {
                            out.push((fp, body));
                        }
                    }
                }
            }
            "variable_declarator" => {
                if let (Some(name_n), Some(val_n)) = (node.child_by_field_name("name"), node.child_by_field_name("value")) {
                    let name = node_text(name_n, src).trim();
                    if name.starts_with("use") && name.chars().nth(3).map_or(false, |c| c.is_ascii_uppercase()) {
                        if matches!(val_n.kind(), "arrow_function" | "function_expression") {
                            if let (Some(fp), Some(body)) = (val_n.child_by_field_name("parameters"), val_n.child_by_field_name("body")) {
                                out.push((fp, body));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for ch in node.named_children(&mut cursor) {
            find_hook_functions(ch, src, out);
        }
    }

    let mut hooks = Vec::new();
    find_hook_functions(root, src, &mut hooks);
    if hooks.is_empty() {
        return None;
    }

    for (fp, body) in hooks {
        let mut array_param_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        for param in named_children_vec(fp) {
            match param.kind() {
                "optional_parameter" | "required_parameter" => {
                    let has_default = param.child_by_field_name("value").is_some();
                    let pat = param.child_by_field_name("pattern")
                        .or_else(|| named_children_vec(param).into_iter().next());
                    let Some(pat_node) = pat else { continue };

                    if pat_node.kind() == "identifier" {
                        let pname = node_text(pat_node, src).trim().to_string();
                        if pname.is_empty() {
                            continue;
                        }
                        let type_ann = param.child_by_field_name("type");
                        let typed_as_array = type_ann.map_or(false, |t| is_array_ts_type(t, src));
                        let usage = analyze_destructured_prop_usage(body, &pname, src);
                        let is_array_param = typed_as_array || usage.used_array_method;

                        if is_array_param {
                            array_param_names.insert(pname.clone());
                            // If optional without default (`fallbackData?: T[]`), replace `?` and add ` = [] as any`
                            if param.kind() == "optional_parameter" && !has_default {
                                let param_txt = node_text(param, src);
                                if let Some(q_rel) = param_txt.find('?') {
                                    let q_abs = param.start_byte() + q_rel;
                                    let q_end = q_abs + 1;
                                    if !overlaps(&touched_ranges, param.start_byte(), param.end_byte()) {
                                        edits.push((q_abs, q_end, String::new()));
                                        edits.push((
                                            param.end_byte(),
                                            param.end_byte(),
                                            if is_ts { " = [] as any".to_string() } else { " = []".to_string() },
                                        ));
                                        touched_ranges.push((param.start_byte(), param.end_byte()));
                                    }
                                }
                            }
                        }
                    } else if pat_node.kind() == "object_pattern" {
                        // Guard destructured array fields inside hook options object (`{ initialSchedules }`)
                        let type_map = param
                            .child_by_field_name("type")
                            .map(|t| extract_object_type_prop_kinds(unwrap_ts_type_node(t), src))
                            .unwrap_or_default();
                        for prop in named_children_vec(pat_node) {
                            match prop.kind() {
                                "shorthand_property_identifier_pattern" => {
                                    let local = node_text(prop, src).trim().to_string();
                                    let typed_arr = matches!(type_map.get(&local), Some(PropTypeKind::Array));
                                    let usage = analyze_destructured_prop_usage(body, &local, src);
                                    if typed_arr || usage.used_array_method {
                                        array_param_names.insert(local);
                                        if !overlaps(&touched_ranges, prop.start_byte(), prop.end_byte()) {
                                            edits.push((prop.end_byte(), prop.end_byte(), " = []".to_string()));
                                            touched_ranges.push((prop.start_byte(), prop.end_byte()));
                                        }
                                    }
                                }
                                "pair_pattern" => {
                                    let key_n = prop.child_by_field_name("key");
                                    let val_n = prop.child_by_field_name("value");
                                    if let (Some(kn), Some(vn)) = (key_n, val_n) {
                                        if vn.kind() == "identifier" {
                                            let key_name = node_text(kn, src).trim().to_string();
                                            let local = node_text(vn, src).trim().to_string();
                                            let typed_arr = matches!(type_map.get(&key_name), Some(PropTypeKind::Array));
                                            let usage = analyze_destructured_prop_usage(body, &local, src);
                                            if typed_arr || usage.used_array_method {
                                                array_param_names.insert(local);
                                                if !overlaps(&touched_ranges, vn.start_byte(), vn.end_byte()) {
                                                    edits.push((vn.end_byte(), vn.end_byte(), " = []".to_string()));
                                                    touched_ranges.push((vn.start_byte(), vn.end_byte()));
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if array_param_names.is_empty() {
            continue;
        }

        // Guard `useState<...>(param)` calls inside the hook body so passing `undefined` or a
        // mismatched non-array argument never initializes array state as undefined/non-array.
        let mut stack = vec![body];
        while let Some(node) = stack.pop() {
            if node.kind() == "call_expression" {
                let func_txt = node.child_by_field_name("function").map(|n| node_text(n, src).trim()).unwrap_or("");
                if func_txt == "useState" || func_txt == "React.useState" {
                    if let Some(args) = node.child_by_field_name("arguments") {
                        let arg_nodes = named_children_vec(args);
                        if arg_nodes.len() == 1 && arg_nodes[0].kind() == "identifier" {
                            let arg_name = node_text(arg_nodes[0], src).trim();
                            if array_param_names.contains(arg_name)
                                && !overlaps(&touched_ranges, arg_nodes[0].start_byte(), arg_nodes[0].end_byte())
                            {
                                let replacement = format!("Array.isArray({}) ? {} : []", arg_name, arg_name);
                                edits.push((arg_nodes[0].start_byte(), arg_nodes[0].end_byte(), replacement));
                                touched_ranges.push((arg_nodes[0].start_byte(), arg_nodes[0].end_byte()));
                            }
                        }
                    }
                }
            }
            let mut cursor = node.walk();
            for ch in node.named_children(&mut cursor) {
                stack.push(ch);
            }
        }
    }

    if edits.is_empty() {
        return None;
    }

    let candidate = apply_byte_edits(src, edits);
    verified_rewrite(rel_path, src, candidate)
}

pub fn ensure_hook_array_param_defaults_in_project(proj_dir: &std::path::Path) -> usize {
    let mut updated = 0usize;
    let mut files = Vec::new();
    for root in ["src/hooks", "hooks", "src/renderer/hooks"] {
        let dir = proj_dir.join(root);
        if dir.is_dir() {
            collect_source_files(&dir, &mut files);
        }
    }
    for abs_path in files {
        let rel_path = abs_path
            .strip_prefix(proj_dir)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let Ok(orig) = std::fs::read_to_string(&abs_path) else { continue };
        if let Some(next) = ensure_hook_array_param_defaults_in_source(&rel_path, &orig) {
            if next != orig && std::fs::write(&abs_path, next).is_ok() {
                updated += 1;
            }
        }
    }
    updated
}

/// Extracts `(model_delegate_lowercase -> set of enum field names)` directly from `prisma/schema.prisma`.
pub fn extract_prisma_model_enum_fields(schema_src: &str) -> std::collections::HashMap<String, std::collections::HashSet<String>> {
    let mut enum_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Ok(enum_re) = regex::Regex::new(r"(?m)^\s*enum\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{") {
        for cap in enum_re.captures_iter(schema_src) {
            if let Some(m) = cap.get(1) {
                enum_names.insert(m.as_str().to_string());
            }
        }
    }
    let mut out: std::collections::HashMap<String, std::collections::HashSet<String>> = std::collections::HashMap::new();
    if enum_names.is_empty() {
        return out;
    }
    if let Ok(model_re) = regex::Regex::new(r"(?ms)^\s*model\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{([^}]*)\}") {
        for cap in model_re.captures_iter(schema_src) {
            let Some(m_name) = cap.get(1).map(|m| m.as_str()) else { continue };
            let Some(body) = cap.get(2).map(|m| m.as_str()) else { continue };
            let mut fields = std::collections::HashSet::new();
            for line in body.lines() {
                let t = line.trim();
                if t.is_empty() || t.starts_with("//") || t.starts_with("@@") {
                    continue;
                }
                let mut parts = t.split_whitespace();
                let Some(fname) = parts.next() else { continue };
                let Some(ftype_raw) = parts.next() else { continue };
                let ftype = ftype_raw.trim_end_matches(&['?', '[', ']'][..]);
                if enum_names.contains(ftype) {
                    fields.insert(fname.to_string());
                }
            }
            if !fields.is_empty() {
                out.insert(m_name.to_ascii_lowercase(), fields);
            }
        }
    }
    out
}

/// Universally relaxes inline Prisma enum fields inside `prisma.<model>.create/update/upsert({ data: { field: expr } })`
/// when `field` is declared as a Prisma `enum` in `prisma/schema.prisma`, preventing TS2322
/// (`Type 'string' is not assignable to type 'Status'/'Priority'`) from Zod `z.string()` payloads.
pub fn relax_prisma_inline_enum_fields_in_source(
    rel_path: &str,
    src: &str,
    model_enum_fields: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> Option<String> {
    if model_enum_fields.is_empty() || !src.contains("prisma.") {
        return None;
    }
    let lower = rel_path.replace('\\', "/").to_ascii_lowercase();
    if !lower.ends_with(".ts") && !lower.ends_with(".tsx") {
        return None;
    }
    let tree = parse_source(rel_path, src)?;
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut touched_ranges: Vec<(usize, usize)> = Vec::new();

    fn overlaps(ranges: &[(usize, usize)], s: usize, e: usize) -> bool {
        ranges.iter().any(|&(rs, re)| s < re && rs < e)
    }

    fn relax_data_object(
        obj: Node,
        src: &str,
        enum_fields: &std::collections::HashSet<String>,
        edits: &mut Vec<(usize, usize, String)>,
        touched: &mut Vec<(usize, usize)>,
    ) {
        if obj.kind() != "object" {
            return;
        }
        for prop in named_children_vec(obj) {
            if prop.kind() == "pair" {
                let Some(key_n) = prop.child_by_field_name("key") else { continue };
                let Some(val_n) = prop.child_by_field_name("value") else { continue };
                let key = strip_string_quotes(node_text(key_n, src));
                if !enum_fields.contains(&key) {
                    continue;
                }
                if matches!(val_n.kind(), "as_expression" | "satisfies_expression" | "null" | "undefined") {
                    continue;
                }
                // Skip explicit `Enum.VALUE` member expressions where the object starts uppercase
                if val_n.kind() == "member_expression" {
                    if let Some(obj_n) = val_n.child_by_field_name("object") {
                        let otxt = node_text(obj_n, src).trim();
                        if otxt.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                            continue;
                        }
                    }
                }
                let val_txt = node_text(val_n, src).trim();
                if val_txt.contains(" as ") {
                    continue;
                }
                if !overlaps(touched, val_n.start_byte(), val_n.end_byte()) {
                    edits.push((val_n.end_byte(), val_n.end_byte(), " as any".to_string()));
                    touched.push((val_n.start_byte(), val_n.end_byte()));
                }
            } else if prop.kind() == "shorthand_property_identifier" {
                let key = node_text(prop, src).trim();
                if enum_fields.contains(key) && !overlaps(touched, prop.start_byte(), prop.end_byte()) {
                    edits.push((prop.start_byte(), prop.end_byte(), format!("{}: {} as any", key, key)));
                    touched.push((prop.start_byte(), prop.end_byte()));
                }
            }
        }
    }

    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression" {
            if let (Some(func_n), Some(args_n)) = (node.child_by_field_name("function"), node.child_by_field_name("arguments")) {
                if func_n.kind() == "member_expression" {
                    let method = func_n.child_by_field_name("property").map(|n| node_text(n, src).trim()).unwrap_or("");
                    if matches!(method, "create" | "update" | "upsert" | "createMany" | "updateMany") {
                        if let Some(target_n) = func_n.child_by_field_name("object") {
                            if target_n.kind() == "member_expression" {
                                let root_obj = target_n.child_by_field_name("object").map(|n| node_text(n, src).trim()).unwrap_or("");
                                let delegate = target_n.child_by_field_name("property").map(|n| node_text(n, src).trim()).unwrap_or("");
                                if root_obj == "prisma" || root_obj.ends_with(".prisma") {
                                    if let Some(enum_fields) = model_enum_fields.get(&delegate.to_ascii_lowercase()) {
                                        if let Some(first_arg) = named_children_vec(args_n).into_iter().next() {
                                            if first_arg.kind() == "object" {
                                                for pair in named_children_vec(first_arg) {
                                                    if pair.kind() == "pair" {
                                                        let k = pair.child_by_field_name("key").map(|n| strip_string_quotes(node_text(n, src))).unwrap_or_default();
                                                        if matches!(k.as_str(), "data" | "create" | "update") {
                                                            if let Some(v) = pair.child_by_field_name("value") {
                                                                if v.kind() == "object" {
                                                                    relax_data_object(v, src, enum_fields, &mut edits, &mut touched_ranges);
                                                                } else if v.kind() == "array" {
                                                                    for elem in named_children_vec(v) {
                                                                        if elem.kind() == "object" {
                                                                            relax_data_object(elem, src, enum_fields, &mut edits, &mut touched_ranges);
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
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
    let candidate = apply_byte_edits(src, edits);
    verified_rewrite(rel_path, src, candidate)
}

pub fn relax_prisma_inline_enum_fields_in_project(proj_dir: &std::path::Path) -> usize {
    let schema_path = proj_dir.join("prisma").join("schema.prisma");
    let Ok(schema_src) = std::fs::read_to_string(&schema_path) else {
        return 0;
    };
    let model_enum_fields = extract_prisma_model_enum_fields(&schema_src);
    if model_enum_fields.is_empty() {
        return 0;
    }
    let mut updated = 0usize;
    let mut files = Vec::new();
    for root in ["src/app/api", "app/api", "src/lib", "src/services", "services"] {
        let dir = proj_dir.join(root);
        if dir.is_dir() {
            collect_source_files(&dir, &mut files);
        }
    }
    for abs_path in files {
        let rel_path = abs_path
            .strip_prefix(proj_dir)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let Ok(orig) = std::fs::read_to_string(&abs_path) else { continue };
        if let Some(next) = relax_prisma_inline_enum_fields_in_source(&rel_path, &orig, &model_enum_fields) {
            if next != orig && std::fs::write(&abs_path, next).is_ok() {
                updated += 1;
            }
        }
    }
    updated
}

/// Extracts `(model_delegate_lowercase -> Vec<declared_field_name>)` from `prisma/schema.prisma`.
pub fn extract_prisma_model_field_names(schema_src: &str) -> std::collections::HashMap<String, Vec<String>> {
    let mut out: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    if let Ok(model_re) = regex::Regex::new(r"(?ms)^\s*model\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{([^}]*)\}") {
        for cap in model_re.captures_iter(schema_src) {
            let Some(m_name) = cap.get(1).map(|m| m.as_str()) else { continue };
            let Some(body) = cap.get(2).map(|m| m.as_str()) else { continue };
            let mut fields = Vec::new();
            for line in body.lines() {
                let t = line.trim();
                if t.is_empty() || t.starts_with("//") || t.starts_with("@@") {
                    continue;
                }
                if let Some(fname) = t.split_whitespace().next() {
                    if fname.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '_') {
                        fields.push(fname.to_string());
                    }
                }
            }
            if !fields.is_empty() {
                out.insert(m_name.to_ascii_lowercase(), fields);
            }
        }
    }
    out
}

/// Universally corrects case-mismatched Prisma `include` / `select` keys (e.g. `include: { doctor: true }`
/// when `prisma/schema.prisma` declares relation `Doctor`, causing TS2561).
pub fn reconcile_prisma_relation_case_in_source(
    rel_path: &str,
    src: &str,
    model_fields: &std::collections::HashMap<String, Vec<String>>,
) -> Option<String> {
    if model_fields.is_empty() || !src.contains("prisma.") {
        return None;
    }
    let lower = rel_path.replace('\\', "/").to_ascii_lowercase();
    if !lower.ends_with(".ts") && !lower.ends_with(".tsx") {
        return None;
    }
    let tree = parse_source(rel_path, src)?;
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut touched: Vec<(usize, usize)> = Vec::new();

    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression" {
            if let (Some(func_n), Some(args_n)) = (node.child_by_field_name("function"), node.child_by_field_name("arguments")) {
                if func_n.kind() == "member_expression" {
                    if let Some(target_n) = func_n.child_by_field_name("object") {
                        if target_n.kind() == "member_expression" {
                            let root_obj = target_n.child_by_field_name("object").map(|n| node_text(n, src).trim()).unwrap_or("");
                            let delegate = target_n.child_by_field_name("property").map(|n| node_text(n, src).trim()).unwrap_or("");
                            if root_obj == "prisma" || root_obj.ends_with(".prisma") {
                                if let Some(fields) = model_fields.get(&delegate.to_ascii_lowercase()) {
                                    if let Some(first_arg) = named_children_vec(args_n).into_iter().next() {
                                        if first_arg.kind() == "object" {
                                            for pair in named_children_vec(first_arg) {
                                                if pair.kind() == "pair" {
                                                    let k = pair.child_by_field_name("key").map(|n| strip_string_quotes(node_text(n, src))).unwrap_or_default();
                                                    if matches!(k.as_str(), "include" | "select") {
                                                        if let Some(obj_v) = pair.child_by_field_name("value") {
                                                            if obj_v.kind() == "object" {
                                                                for inc_pair in named_children_vec(obj_v) {
                                                                    if inc_pair.kind() == "pair" {
                                                                        let Some(ik_node) = inc_pair.child_by_field_name("key") else { continue };
                                                                        let ik_raw = node_text(ik_node, src).trim();
                                                                        if ik_raw.starts_with('"') || ik_raw.starts_with('\'') || ik_raw.starts_with('_') {
                                                                            continue;
                                                                        }
                                                                        if fields.iter().any(|f| f == ik_raw) {
                                                                            continue;
                                                                        }
                                                                        if let Some(matched) = fields.iter().find(|f| f.eq_ignore_ascii_case(ik_raw)) {
                                                                            if !touched.iter().any(|&(rs, re)| ik_node.start_byte() < re && rs < ik_node.end_byte()) {
                                                                                edits.push((ik_node.start_byte(), ik_node.end_byte(), matched.clone()));
                                                                                touched.push((ik_node.start_byte(), ik_node.end_byte()));
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
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
    let candidate = apply_byte_edits(src, edits);
    verified_rewrite(rel_path, src, candidate)
}

pub fn reconcile_prisma_relation_case_in_project(proj_dir: &std::path::Path) -> usize {
    let schema_path = proj_dir.join("prisma").join("schema.prisma");
    let Ok(schema_src) = std::fs::read_to_string(&schema_path) else {
        return 0;
    };
    let model_fields = extract_prisma_model_field_names(&schema_src);
    if model_fields.is_empty() {
        return 0;
    }
    let mut updated = 0usize;
    let mut files = Vec::new();
    for root in ["src/app/api", "app/api", "src/lib", "src/services", "services"] {
        let dir = proj_dir.join(root);
        if dir.is_dir() {
            collect_source_files(&dir, &mut files);
        }
    }
    for abs_path in files {
        let rel_path = abs_path
            .strip_prefix(proj_dir)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let Ok(orig) = std::fs::read_to_string(&abs_path) else { continue };
        if let Some(next) = reconcile_prisma_relation_case_in_source(&rel_path, &orig, &model_fields) {
            if next != orig && std::fs::write(&abs_path, next).is_ok() {
                updated += 1;
            }
        }
    }
    updated
}

/// Removes dead `import ... from '<missing_module>'` statements (e.g. TS2307) when NONE of the
/// imported local bindings are referenced anywhere outside import statements in the AST.
pub fn remove_unused_missing_module_imports(
    path: &str,
    src: &str,
    missing_modules: &[String],
) -> Option<String> {
    if missing_modules.is_empty() {
        return None;
    }
    let tree = parse_source(path, src)?;

    let mut used_idents = std::collections::HashSet::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "import_statement" {
            continue;
        }
        if matches!(
            node.kind(),
            "identifier"
                | "type_identifier"
                | "jsx_identifier"
                | "shorthand_property_identifier"
                | "shorthand_property_identifier_pattern"
        ) {
            let txt = node_text(node, src).trim();
            if !txt.is_empty() {
                used_idents.insert(txt.to_string());
            }
        }
        let mut cursor = node.walk();
        for ch in node.named_children(&mut cursor) {
            stack.push(ch);
        }
    }

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for stmt in named_children_vec(tree.root_node()) {
        if stmt.kind() != "import_statement" {
            continue;
        }
        let Some(source) = stmt
            .child_by_field_name("source")
            .or_else(|| first_named_of_kind(stmt, &["string"]))
        else {
            continue;
        };
        let module_spec = strip_string_quotes(node_text(source, src));
        if !missing_modules.iter().any(|m| m == &module_spec) {
            continue;
        }
        let Some(clause) = first_named_of_kind(stmt, &["import_clause"]) else {
            continue;
        };
        let mut locals = Vec::new();
        for part in named_children_vec(clause) {
            match part.kind() {
                "identifier" => locals.push(node_text(part, src).trim().to_string()),
                "namespace_import" => {
                    for ch in named_children_vec(part) {
                        if ch.kind() == "identifier" {
                            locals.push(node_text(ch, src).trim().to_string());
                        }
                    }
                }
                "named_imports" => {
                    for spec in named_children_vec(part) {
                        if spec.kind() == "import_specifier" {
                            let local_n = spec.child_by_field_name("alias")
                                .or_else(|| spec.child_by_field_name("name"))
                                .or_else(|| named_children_vec(spec).into_iter().next());
                            if let Some(ln) = local_n {
                                locals.push(strip_string_quotes(node_text(ln, src)));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if !locals.is_empty() && locals.iter().all(|l| !used_idents.contains(l)) {
            let mut del_end = stmt.end_byte();
            while del_end < src.len() && matches!(src.as_bytes()[del_end], b' ' | b'\t') {
                del_end += 1;
            }
            if src[del_end..].starts_with("\r\n") {
                del_end += 2;
            } else if src[del_end..].starts_with('\n') {
                del_end += 1;
            }
            edits.push((stmt.start_byte(), del_end, String::new()));
        }
    }

    if edits.is_empty() {
        return None;
    }
    let candidate = apply_byte_edits(src, edits);
    verified_rewrite(path, src, candidate)
}

/// Adds a companion `export type Name = (typeof Name)[keyof typeof Name] | string;`
/// when a TypeScript module exports a PascalCase enum-like `const Name = { KEY: 'VAL', ... }`
/// object literal without an accompanying type/interface/enum declaration (`TS2749`).
pub fn ensure_companion_types_for_const_enums_in_source(rel_path: &str, source: &str) -> Option<String> {
    use std::collections::HashSet;

    if !is_typed_path(rel_path) {
        return None;
    }
    let low = rel_path.replace('\\', "/").to_ascii_lowercase();
    if low.ends_with(".d.ts") || low == "src/types/index.ts" || low == "types/index.ts" {
        return None;
    }
    if !source.contains("export const ") {
        return None;
    }

    let tree = parse_source(rel_path, source)?;
    let mut existing_type_names: HashSet<String> = HashSet::new();

    for imp in extract_imports(&tree, source) {
        if let Some(d) = imp.default_local {
            existing_type_names.insert(d);
        }
        if let Some(ns) = imp.namespace_local {
            existing_type_names.insert(ns);
        }
        for b in imp.named {
            existing_type_names.insert(b.local);
        }
    }

    for stmt in named_children_vec(tree.root_node()) {
        let decl = if stmt.kind() == "export_statement" {
            match stmt.child_by_field_name("declaration") {
                Some(d) => d,
                None => continue,
            }
        } else {
            stmt
        };
        if matches!(
            decl.kind(),
            "type_alias_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "class_declaration"
                | "abstract_class_declaration"
        ) {
            if let Some(name_node) = decl.child_by_field_name("name") {
                let n = node_text(name_node, source).trim();
                if !n.is_empty() {
                    existing_type_names.insert(n.to_string());
                }
            }
        }
    }

    let excluded_suffixes = [
        "Props", "Schema", "Config", "Options", "Context", "Provider", "Client", "Router",
        "Store", "Mock", "Data", "Items", "List", "Map", "Styles", "Colors", "Theme",
    ];

    let mut edits: Vec<(usize, usize, String)> = Vec::new();

    for stmt in named_children_vec(tree.root_node()) {
        if stmt.kind() != "export_statement" || has_token_child(stmt, "default") {
            continue;
        }
        let Some(decl) = stmt.child_by_field_name("declaration") else { continue };
        if decl.kind() != "lexical_declaration" || !has_token_child(decl, "const") {
            continue;
        }

        let mut stmt_types: Vec<String> = Vec::new();
        for var_decl in named_children_vec(decl) {
            if var_decl.kind() != "variable_declarator" {
                continue;
            }
            let Some(name_node) = var_decl.child_by_field_name("name") else { continue };
            if name_node.kind() != "identifier" {
                continue;
            }
            let name = node_text(name_node, source).trim();
            if name.len() < 3
                || !name.chars().next().map_or(false, |c| c.is_ascii_uppercase())
                || !name.chars().any(|c| c.is_ascii_lowercase())
                || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                || excluded_suffixes.iter().any(|suf| name.ends_with(suf))
                || existing_type_names.contains(name)
            {
                continue;
            }

            let Some(mut val) = var_decl.child_by_field_name("value") else { continue };
            while matches!(
                val.kind(),
                "as_expression" | "satisfies_expression" | "parenthesized_expression"
            ) {
                match named_children_vec(val).into_iter().next() {
                    Some(inner) => val = inner,
                    None => break,
                }
            }
            if val.kind() != "object" {
                continue;
            }

            let members = named_children_vec(val);
            if members.len() < 2 {
                continue;
            }
            let all_upper_string_pairs = members.iter().all(|m| {
                if m.kind() != "pair" {
                    return false;
                }
                let Some(k_node) = m.child_by_field_name("key") else { return false };
                let Some(v_node) = m.child_by_field_name("value") else { return false };
                let Some(key_name) = property_key_name(k_node, source) else { return false };
                key_name.len() >= 2
                    && key_name.chars().next().map_or(false, |c| c.is_ascii_uppercase())
                    && key_name
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                    && v_node.kind() == "string"
            });

            if all_upper_string_pairs {
                existing_type_names.insert(name.to_string());
                stmt_types.push(format!(
                    "export type {} = (typeof {})[keyof typeof {}] | string;",
                    name, name, name
                ));
            }
        }

        if !stmt_types.is_empty() {
            edits.push((
                stmt.end_byte(),
                stmt.end_byte(),
                format!("\n{}\n", stmt_types.join("\n")),
            ));
        }
    }

    if edits.is_empty() {
        return None;
    }
    let candidate = apply_byte_edits(source, edits);
    verified_rewrite(rel_path, source, candidate)
}
