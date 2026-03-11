# Dashboard UI Architecture

How maud, HTMX, Alpine.js, and Pico CSS work together in Thunderbot's dashboard.

## Stack Roles

| Tool          | Role                                                                   | Scope                                    |
| ------------- | ---------------------------------------------------------------------- | ---------------------------------------- |
| **maud**      | Server-side HTML generation via Rust macros                            | All HTML output                          |
| **HTMX**      | Server-driven interactions (fetch fragments, polling, form submission) | Data fetching, live updates, navigation  |
| **Alpine.js** | Client-side UI state (toggles, modals, tabs, filters)                  | Local interactivity that needs no server |
| **Pico CSS**  | Base styling via semantic HTML + oxocarbon overrides                   | Look and feel                            |

**Decision rule**: If it needs server data, use HTMX. If it's pure UI state (open/close, tab switch, filter pill), use Alpine. Never use Alpine to fetch data; never use HTMX for local toggles.

## Maud Patterns

### Returning HTML from Axum

Markup auto-implements IntoResponse with text/html content type.

### Syntax Quick Reference

```rust
html! {
    // Elements
    div class="card" data-status="ok" { "content" }
    br;  // void elements use ;

    // Splices (auto-escaped)
    p { (variable) }
    a href=(url) { "link" }

    // Raw HTML (skip escaping)
    script { (PreEscaped("console.log('hi')")) }

    // Control flow — all prefixed with @
    @if condition { p { "yes" } }
    @for item in &items { li { (item.name) } }
    @if let Some(v) = opt { span { (v) } }
    @match status {
        Status::Ok => span.green { "ok" },
        _ => span.red { "error" },
    }

    // Optional attributes
    input checked[is_active];  // boolean toggle
    p title=[maybe_tooltip];   // Option<&str> — omitted when None
}
```

## HTMX Patterns

### Fragment Responses

HTMX requests return **bare HTML fragments**, not full pages. The server detects HTMX via the `HX-Request` header.

### Polling

HTMX can poll for updates via the `HX-Poll` attribute.

#### Stop Polling

Return HTTP 286 from the handler to tell HTMX to stop a polling trigger.

### Out-of-Band (OOB) Updates

A single response can update multiple DOM targets.

## Alpine.js Patterns

### Where Alpine Lives in Maud

Alpine attributes render as normal HTML attributes in maud.

**Note**: `@click` needs quoting in maud because `@` is a control character. Use `"@click"="..."`.

### Alpine `$store` for Cross-Component State

Use for state that must survive HTMX DOM swaps (theme, sidebar, notifications).

## Pico CSS Patterns

### Semantic HTML — Pico Styles It

Pico styles native elements. No class needed for basic components:

| Element                                | Pico Behavior                         |
| -------------------------------------- | ------------------------------------- |
| `<article>`                            | Card with padding, border, background |
| `<header>/<footer>` inside `<article>` | Separator + spacing                   |
| `<nav> > ul`                           | Horizontal nav items                  |
| `<nav>` inside `<aside>`               | Vertical nav items                    |
| `<table>`                              | Clean minimal table                   |
| `<dialog>`                             | Modal overlay                         |
| `<details class="dropdown">`           | Dropdown menu                         |
| `<form role="search">`                 | Inline search bar                     |
| `<div role="group">`                   | Button group (segmented control)      |
| `<hgroup>`                             | Title + muted subtitle                |

- Pico's `.grid` — equal auto-columns, collapses below 768px
- Pico's `.flex` — flexbox with gap, collapses below 768px
- Pico's `.stack` — flexbox with gap, stacks below 768px
- Pico auto-styles `<article>` as a card, `<header>` gets a bottom separator, `<h3>` is prominent.
