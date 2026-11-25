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
use crate::error::PageReadError;

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

    pub(super) fn parse(&self, html: &str) -> Result<Html, PageReadError> {
        self.before_deadline()?;
        markup::check(html).map_err(|_| PageReadError::MarkupLimit)?;
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
        .map_err(|failure| match failure.downcast::<LimitExceeded>() {
            Ok(limit) => limit.0,
            Err(failure) => resume_unwind(failure),
        })?;
        self.before_deadline()?;
        Ok(document)
    }

    fn before_deadline(&self) -> Result<(), PageReadError> {
        if Instant::now() < self.deadline {
            Ok(())
        } else {
            Err(PageReadError::Timeout)
        }
    }

    fn work(&self, count: usize) {
        let before = self.operations.get();
        let after = debit(&self.operations, count, PageReadError::DomOperations);
        // Check time periodically even in tree-builder comparison loops, without
        // querying the clock on every ordinary operation.
        if before / 256 != after / 256
            && let Err(reason) = self.before_deadline()
        {
            stop(reason);
        }
    }

    fn allocate(&self, items: usize, bytes: usize) {
        self.charge(items, bytes, 1 + items);
    }

    fn charge(&self, items: usize, bytes: usize, operations: usize) {
        debit(&self.items, items, PageReadError::DomItems);
        debit(&self.bytes, bytes, PageReadError::DomBytes);
        self.work(operations);
    }
}

struct LimitExceeded(PageReadError);

fn stop(reason: PageReadError) -> ! {
    // A rejected document is expected; bypass the process-wide panic hook.
    resume_unwind(Box::new(LimitExceeded(reason)))
}

fn debit(remaining: &Cell<usize>, count: usize, reason: PageReadError) -> usize {
    let after = remaining
        .get()
        .checked_sub(count)
        .unwrap_or_else(|| stop(reason));
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
    fn insert(&self, child: &Child, creates_text_node: impl FnOnce() -> bool) {
        match child {
            NodeOrText::AppendNode(_) => self.budget.work(1),
            NodeOrText::AppendText(text) => {
                // HtmlTreeSink coalesces adjacent text. Even when no new node
                // is needed, charge the full byte and operation cost.
                self.budget
                    .charge(usize::from(creates_text_node()), text.len(), 2);
            }
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
        self.insert(&child, || {
            let document = self.inner.0.borrow();
            let parent = document.tree.get(*parent).unwrap();
            !parent.last_child().is_some_and(|n| n.value().is_text())
        });
        self.inner.append(parent, child);
    }

    fn append_based_on_parent_node(&self, element: &Node, previous: &Node, child: Child) {
        // Use the same foster-parent choice as HtmlTreeSink, so the chosen
        // insertion site determines whether this text will coalesce.
        let has_parent = {
            let document = self.inner.0.borrow();
            document.tree.get(*element).unwrap().parent().is_some()
        };
        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(previous, child);
        }
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
        self.insert(&child, || {
            let document = self.inner.0.borrow();
            let sibling = document.tree.get(*sibling).unwrap();
            sibling.parent().is_some()
                && !sibling.prev_sibling().is_some_and(|n| n.value().is_text())
        });
        self.inner.append_before_sibling(sibling, child);
    }

    fn add_attrs_if_missing(&self, node: &Node, attrs: Vec<Attribute>) {
        // Cap cumulative attempts before the inner sorted-vector insertion.
        // Duplicate names are charged too: they still require a lookup.
        debit(
            &self.budget.merges,
            attrs.len(),
            PageReadError::AttributeMerges,
        );
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
