use std::{
    borrow::Cow,
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    time::Instant,
};

use html5ever::{
    Attribute, QualName, parse_document,
    tendril::{StrTendril, TendrilSink},
    tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink},
};
use scraper::{Html, HtmlTreeSink};

use super::markup;

#[cfg(not(panic = "unwind"))]
compile_error!("Suggestion parsing requires panic unwinding to stop html5ever safely");

const DOM_ITEMS: usize = 8192;
const DOM_BYTES: usize = 2 * 1024 * 1024;
const DOM_OPERATIONS: usize = 256 * 1024;
const ATTRIBUTE_MERGES: usize = 128;

pub(super) struct Budget {
    items: Cell<usize>,
    bytes: Cell<usize>,
    operations: Cell<usize>,
    merges: Cell<usize>,
    deadline: Instant,
}

impl Budget {
    pub(super) fn new(deadline: Instant) -> Self {
        Self {
            items: Cell::new(DOM_ITEMS),
            bytes: Cell::new(DOM_BYTES),
            operations: Cell::new(DOM_OPERATIONS),
            merges: Cell::new(ATTRIBUTE_MERGES),
            deadline,
        }
    }

    pub(super) fn parse(&self, html: &str) -> Result<Html, ()> {
        self.before_deadline()?;
        markup::check(html)?;
        self.before_deadline()?;
        // TreeSink callbacks cannot return errors. Unwind only to this local
        // boundary, before an over-budget operation reaches the real sink.
        let document = catch_unwind(AssertUnwindSafe(|| {
            let inner = HtmlTreeSink::new(Html::new_document());
            parse_document(
                Sink {
                    inner,
                    budget: self,
                },
                Default::default(),
            )
            .one(html)
        }))
        .or_else(|failure| {
            if failure.is::<LimitExceeded>() {
                Err(())
            } else {
                resume_unwind(failure)
            }
        })?;
        self.before_deadline()?;
        Ok(document)
    }

    fn before_deadline(&self) -> Result<(), ()> {
        if Instant::now() < self.deadline {
            Ok(())
        } else {
            Err(())
        }
    }

    fn work(&self, count: usize) {
        let before = self.operations.get();
        let after = debit(&self.operations, count);
        // Check time periodically even in tree-builder comparison loops, without
        // querying the clock on every ordinary operation.
        if before / 256 != after / 256 && self.before_deadline().is_err() {
            stop();
        }
    }

    fn allocate(&self, items: usize, bytes: usize) {
        debit(&self.items, items);
        debit(&self.bytes, bytes);
        self.work(1 + items);
    }
}

struct LimitExceeded;

fn stop() -> ! {
    // A rejected document is expected; bypass the process-wide panic hook.
    resume_unwind(Box::new(LimitExceeded))
}

fn debit(remaining: &Cell<usize>, count: usize) -> usize {
    let after = remaining.get().checked_sub(count).unwrap_or_else(|| stop());
    remaining.set(after);
    after
}

pub(super) type Node = <HtmlTreeSink as TreeSink>::Handle;
type Child = NodeOrText<Node>;

struct Sink<'a> {
    inner: HtmlTreeSink,
    budget: &'a Budget,
}

impl Sink<'_> {
    fn insert(&self, child: &Child) {
        match child {
            NodeOrText::AppendNode(_) => self.budget.work(1),
            NodeOrText::AppendText(text) => self.budget.allocate(1, text.len()),
        }
    }
}

fn attribute_bytes(attrs: &[Attribute]) -> usize {
    attrs
        .iter()
        .map(|attr| attr.name.local.len() + attr.value.len())
        .sum()
}

impl TreeSink for Sink<'_> {
    type Handle = Node;
    type Output = Html;
    type ElemName<'a>
        = <HtmlTreeSink as TreeSink>::ElemName<'a>
    where
        Self: 'a;

    fn finish(self) -> Html {
        self.inner.finish()
    }

    fn parse_error(&self, message: Cow<'static, str>) {
        self.budget.allocate(0, message.len());
        self.inner.parse_error(message);
    }

    fn get_document(&self) -> Node {
        self.inner.get_document()
    }

    fn elem_name<'a>(&'a self, node: &'a Node) -> Self::ElemName<'a> {
        self.budget.work(1);
        self.inner.elem_name(node)
    }

    fn create_element(&self, name: QualName, attrs: Vec<Attribute>, flags: ElementFlags) -> Node {
        // Includes synthetic and reconstructed nodes, not just source tags.
        self.budget.allocate(
            1 + usize::from(flags.template) + attrs.len(),
            name.local.len() + attribute_bytes(&attrs),
        );
        self.inner.create_element(name, attrs, flags)
    }

    fn create_comment(&self, text: StrTendril) -> Node {
        self.budget.allocate(1, text.len());
        self.inner.create_comment(text)
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Node {
        self.budget.allocate(1, target.len() + data.len());
        self.inner.create_pi(target, data)
    }

    fn append(&self, parent: &Node, child: Child) {
        self.insert(&child);
        self.inner.append(parent, child);
    }

    fn append_based_on_parent_node(&self, element: &Node, previous: &Node, child: Child) {
        self.insert(&child);
        self.inner
            .append_based_on_parent_node(element, previous, child);
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        self.budget
            .allocate(1, name.len() + public_id.len() + system_id.len());
        self.inner
            .append_doctype_to_document(name, public_id, system_id);
    }

    fn pop(&self, _: &Node) {
        self.budget.work(1);
    }

    fn get_template_contents(&self, node: &Node) -> Node {
        self.budget.work(1);
        self.inner.get_template_contents(node)
    }

    fn same_node(&self, left: &Node, right: &Node) -> bool {
        self.budget.work(1);
        self.inner.same_node(left, right)
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.inner.set_quirks_mode(mode);
    }

    fn append_before_sibling(&self, sibling: &Node, child: Child) {
        self.insert(&child);
        self.inner.append_before_sibling(sibling, child);
    }

    fn add_attrs_if_missing(&self, node: &Node, attrs: Vec<Attribute>) {
        // Cap cumulative attempts before the inner sorted-vector insertion.
        // Duplicate names are charged too: they still require a lookup.
        debit(&self.budget.merges, attrs.len());
        self.budget.allocate(attrs.len(), attribute_bytes(&attrs));
        self.inner.add_attrs_if_missing(node, attrs);
    }

    fn remove_from_parent(&self, node: &Node) {
        self.budget.work(1);
        self.inner.remove_from_parent(node);
    }

    fn reparent_children(&self, node: &Node, parent: &Node) {
        // The sink walks the children; charge the number of allocated items
        // as a conservative upper bound without walking the siblings twice.
        self.budget.work(1 + DOM_ITEMS - self.budget.items.get());
        self.inner.reparent_children(node, parent);
    }
}
