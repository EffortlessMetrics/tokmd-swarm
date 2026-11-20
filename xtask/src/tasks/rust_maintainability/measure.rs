use super::{Identity, Metrics, Symbol};
use quote::ToTokens;
use syn::{
    Attribute, Block, Expr, FnArg, ImplItem, Item, Signature, Stmt, TraitItem, visit::Visit,
};

// Ignore attributes whose semantics could change the compiled callable population.
// Doc comments do not change admission.
pub(super) fn admitted(attrs: &[Attribute]) -> bool {
    attrs.iter().all(|attr| attr.path().is_ident("doc"))
}

pub(super) fn symbols(file: &syn::File, byte_offset: usize) -> Vec<Symbol> {
    let mut collector = Collector {
        byte_offset,
        ..Default::default()
    };
    for item in &file.items {
        collector.item(item);
    }
    collector
        .symbols
        .sort_by(|a, b| (&a.id, a.lines).cmp(&(&b.id, b.lines)));
    collector.symbols
}

#[derive(Default)]
struct Collector {
    byte_offset: usize,
    owners: Vec<String>,
    symbols: Vec<Symbol>,
}

impl Collector {
    fn item(&mut self, item: &Item) {
        match item {
            Item::Fn(f) if admitted(&f.attrs) => self.function(&f.sig, Some(&f.block)),
            Item::Mod(m) if admitted(&m.attrs) => {
                if let Some((_, items)) = &m.content {
                    self.owners.push(format!("mod {}", m.ident));
                    for item in items {
                        self.item(item);
                    }
                    self.owners.pop();
                }
            }
            Item::Impl(i) if admitted(&i.attrs) => {
                let mut header = i.clone();
                header.items.clear();
                header.attrs.clear();
                self.owners.push(header.to_token_stream().to_string());
                for item in &i.items {
                    if let ImplItem::Fn(f) = item
                        && admitted(&f.attrs)
                    {
                        self.function(&f.sig, Some(&f.block));
                    }
                }
                self.owners.pop();
            }
            Item::Trait(t) if admitted(&t.attrs) => {
                let mut header = t.clone();
                header.items.clear();
                header.attrs.clear();
                self.owners.push(header.to_token_stream().to_string());
                for item in &t.items {
                    if let TraitItem::Fn(f) = item
                        && admitted(&f.attrs)
                    {
                        self.function(&f.sig, f.default.as_ref());
                    }
                }
                self.owners.pop();
            }
            _ => {}
        }
    }

    fn function(&mut self, sig: &Signature, body: Option<&Block>) {
        let name = sig.ident.to_string();
        let signature = sig.to_token_stream().to_string();
        let mut anonymous = sig.clone();
        anonymous.ident = syn::Ident::new("__callable", sig.ident.span());
        let fingerprint = blake3::hash(quote::quote!(#anonymous #body).to_string().as_bytes());
        let mut depth = ControlDepth::default();
        if let Some(block) = body {
            depth.visit_block(block);
        }
        let parameters = sig
            .inputs
            .iter()
            .filter(|arg| matches!(arg, FnArg::Typed(_)))
            .count()
            + usize::from(sig.variadic.is_some());
        let start = sig.fn_token.span;
        let end = body.map_or_else(
            || syn::spanned::Spanned::span(sig),
            |b| b.brace_token.span.close(),
        );
        self.symbols.push(Symbol {
            id: Identity {
                owners: self.owners.clone(),
                name,
                signature: signature.clone(),
            },
            lines: [start.start().line, end.end().line],
            bytes: [
                self.byte_offset + start.byte_range().start,
                self.byte_offset + end.byte_range().end,
            ],
            fingerprint: fingerprint.to_hex().to_string(),
            metrics: Metrics {
                parameters,
                nesting: body.filter(|_| !depth.opaque).map(|_| depth.max),
            },
        });
        if let Some(block) = body {
            self.owners.push(signature);
            for stmt in &block.stmts {
                if let Stmt::Item(item) = stmt {
                    self.item(item);
                }
            }
            self.owners.pop();
        }
    }
}

#[derive(Default)]
struct ControlDepth {
    opaque: bool,
    current: usize,
    max: usize,
}

impl<'ast> Visit<'ast> for ControlDepth {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        if matches!(expr, Expr::Closure(_) | Expr::Async(_) | Expr::Const(_)) {
            self.opaque = true;
            return;
        }
        let control = usize::from(matches!(
            expr,
            Expr::If(_) | Expr::ForLoop(_) | Expr::Loop(_) | Expr::Match(_) | Expr::While(_)
        ));
        self.current += control;
        self.max = self.max.max(self.current);
        syn::visit::visit_expr(self, expr);
        self.current -= control;
    }

    fn visit_macro(&mut self, _: &'ast syn::Macro) {
        self.opaque = true;
    }

    fn visit_item(&mut self, _: &'ast Item) {}
}
