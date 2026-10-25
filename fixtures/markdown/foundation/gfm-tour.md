---
description: A tour of GFM features
author: mdv examples
tags:
  - gfm
  - demo
draft: false
---

# GFM feature tour

A quick walk through the GitHub Flavored Markdown features that mdv renders. Jump to [the table section](#tables) or [code blocks](#fenced-code).

## Emphasis

This is *italic*, **bold**, ***bold italic***, and ~~strikethrough~~. Inline `code` looks like this.

## Lists

- Apples
- Oranges
  - Blood oranges
  - Navel oranges
- Pears

1. First
2. Second
3. Third

### Task list

- [x] Write the example files
- [x] Check the links
- [ ] Take screenshots for the docs

## Tables

| Name   | Type    | Score |
| :----- | :-----: | ----: |
| Alpha  | string  |    10 |
| Beta   | number  |   200 |
| Gamma  | boolean |     3 |

## Blockquotes

> Simple things should be simple.
>
> > Nested quotes work too.

## Alerts

<!-- markdownlint-disable MD028 -->

> [!NOTE]
> Useful information that users should know, even when skimming.

> [!TIP]
> Helpful advice for doing things better or more easily.

> [!IMPORTANT]
> Key information users need to know to achieve their goal.

> [!WARNING]
> Urgent info that needs immediate user attention to avoid problems.

> [!CAUTION]
> Advises about risks or negative outcomes of certain actions.

<!-- markdownlint-enable MD028 -->

## Math

Inline math such as $e^{i\pi} + 1 = 0$ sits in the text. Display math:

$$
\int_{-\infty}^{\infty} e^{-x^2}\,dx = \sqrt{\pi}
$$

A `math` fence works too:

```math
\begin{pmatrix} a & b \\ c & d \end{pmatrix}
```

Escape a literal dollar sign as `\$`, for example \$5.

## Fenced code

```go
package main

import "fmt"

func main() {
    fmt.Println("hello from mdv")
}
```

```python
def greet(name: str) -> str:
    return f"Hello, {name}!"
```

```typescript
interface User {
  id: number;
  name: string;
}

const user: User = { id: 1, name: "Ada" };
```

```bash
for f in *.md; do
  echo "found $f"
done
```

```json
{ "name": "mdv", "private": true }
```

```yaml
roots:
  - name: examples
    path: ./examples
```

## Links

Autolink: <https://example.com> is linked automatically.

Back to the [examples index](README.md), or see the [Mermaid page](mermaid.md#sequence-diagram).

## Footnotes

Footnotes keep the main text clean.[^1] They can appear anywhere.[^note]

[^1]: This is the first footnote.
[^note]: A named footnote with `code` in it.

## Horizontal rule

---

## Collapsible HTML

<details>
<summary>Click to expand</summary>

Hidden content, with **Markdown** inside.

</details>

Jump back to the [top](#gfm-feature-tour).
