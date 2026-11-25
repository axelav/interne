(() => {
    const input = document.getElementById('tags');
    const list = document.getElementById('tag-suggestions');
    let segment = null;
    let timer;
    let request;
    let selected = null;
    let selectionEnter = false;
    let composing = false;

    function currentSegment() {
        const { value, selectionStart, selectionEnd } = input;
        const start = selectionStart === 0 ? 0 : value.lastIndexOf(',', selectionStart - 1) + 1;
        const comma = value.indexOf(',', selectionStart);
        const end = comma === -1 ? value.length : comma;
        return selectionEnd <= end ? { value, start, end } : null;
    }

    function sameSegment(a, b) {
        return a === b || (a && b && a.value === b.value && a.start === b.start && a.end === b.end);
    }

    function dismiss() {
        clearTimeout(timer);
        request?.abort();
        selected = null;
        list.hidden = true;
        list.replaceChildren();
        input.setAttribute('aria-expanded', 'false');
        input.removeAttribute('aria-activedescendant');
    }

    function refresh() {
        if (composing || document.activeElement !== input) return;
        const next = currentSegment();
        if (sameSegment(next, segment)) return;
        segment = next;
        dismiss();
        if (!next) return;
        const query = next.value.slice(next.start, next.end);
        if (!/\P{White_Space}/u.test(query)) return;

        const { signal } = request = new AbortController();
        timer = setTimeout(async () => {
            try {
                const exclude = next.value.slice(0, next.start) + next.value.slice(next.end + 1);
                const params = new URLSearchParams({ q: query });
                if (exclude) params.set('exclude', exclude);
                const response = await fetch('/api/tags/search?' + params, {
                    signal, redirect: 'error',
                });
                if (!response.ok) return;
                const html = await response.text();
                if (signal.aborted || !sameSegment(next, currentSegment())) return;

                list.innerHTML = html;
                list.hidden = list.children.length === 0;
                input.setAttribute('aria-expanded', String(!list.hidden));
            } catch (error) {
                if (!signal.aborted) console.warn('Tag lookup unavailable', error);
            }
        }, 300);
    }

    function choose(option) {
        if (!option || !sameSegment(segment, currentSegment())) return;
        const { value, start, end } = segment;
        const text = value.slice(start, end);
        const from = start + text.match(/^\p{White_Space}*/u)[0].length;
        const to = end - text.match(/\p{White_Space}*$/u)[0].length;
        input.setRangeText(option.textContent, from, to, 'end');
        if (end === value.length) {
            input.setRangeText(', ', input.value.length, input.value.length, 'end');
        }
        segment = currentSegment();
        dismiss();
    }

    input.addEventListener('input', refresh);
    input.addEventListener('selectionchange', refresh);
    input.addEventListener('compositionstart', () => {
        composing = true;
        segment = null;
        dismiss();
    });
    input.addEventListener('compositionend', () => {
        composing = false;
        refresh();
    });
    input.addEventListener('blur', () => {
        segment = null;
        selectionEnter = false;
        dismiss();
    });

    input.addEventListener('keydown', event => {
        if (event.isComposing) return;
        if (event.key === 'Escape') {
            event.preventDefault();
            dismiss();
        } else if (event.key === 'Enter') {
            if (event.repeat && selectionEnter) {
                event.preventDefault();
                return;
            }
            selectionEnter = selected !== null;
            if (selectionEnter) {
                event.preventDefault();
                choose(selected);
            }
        } else if (!list.hidden && sameSegment(segment, currentSegment()) &&
            (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
            event.preventDefault();
            selected?.setAttribute('aria-selected', 'false');
            selected = event.key === 'ArrowDown'
                ? selected?.nextElementSibling ?? list.firstElementChild
                : selected?.previousElementSibling ?? list.lastElementChild;
            selected.setAttribute('aria-selected', 'true');
            input.setAttribute('aria-activedescendant', selected.id);
            selected.scrollIntoView({ block: 'nearest' });
        }
    });

    list.addEventListener('mousedown', event => {
        if (event.button === 0 && event.target.closest('[role="option"]')) event.preventDefault();
    });
    list.addEventListener('click', event => choose(event.target.closest('[role="option"]')));
})();
