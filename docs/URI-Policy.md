# V2 URI and resource policy

CORE-003B2 adds an opt-in pure classifier and validated typed intents under
[model v2](../specs/semantic-model/v2/README.md). V1's conservative parser policy,
wire/schema and default API remain intact. Neither version opens files, follows
links, performs DNS/network requests, invokes a shell or grants permission.

## Value boundary

V2 keeps original parser-decoded destination text and exact inline/reference
source provenance. Its target is one of:

| Target | Meaning |
| --- | --- |
| `fragment` | Decoded anchor within the same document, possibly unresolved |
| `requires_capability` | UTF-8 relative file path decoded exactly once, plus optional decoded fragment |
| `external` | Canonical http/https or limited mailto URI for explicit platform activation |
| `denied` | Structural reason: unsafe, invalid URI, unsupported scheme, remote resource, limit or retained legacy denial |

Only source-proven, policy-consistent actionable targets validate. Caller-supplied
stricter denials are allowed; wire data cannot replace a safe source destination
with another self-consistent actionable destination. Canonical target text is
not the original source lexeme and must not be presented as such.

## Common guards

A raw destination is at most 8,192 UTF-8 bytes. Larger requests remain denied;
they do not invoke URL normalization. Reject raw leading/trailing whitespace,
controls, Unicode 16.0 Cf formats, Default_Ignorable_Code_Point values (Hangul
fillers, CGJ, variation selectors, Mongolian FVS, tags), U+2800 braille blank,
non-space whitespace, backslashes, malformed
percent triplets and invalid percent-decoded UTF-8. Apply control/format checks
after one strict percent decode as well, before IDNA/URL normalization could
remove or reinterpret them. Ordinary Unicode/NFD file names and internal spaces
are supported. This is not a full Unicode confusable/grapheme validator.
Rejecting default-ignorable values denies rare legitimate targets such as emoji
with U+FE0F; display text keeps them. The pinned set comes from
[Unicode 16.0 DerivedCoreProperties](https://www.unicode.org/Public/16.0.0/ucd/DerivedCoreProperties.txt),
cross-checked against the locked icu_properties_data 2.4.0 (Unicode 18.0, same
set), under the [Unicode License V3](../licenses/Unicode-3.0.txt).

Scheme recognition only considers a colon before slash/query/fragment boundaries;
a colon within a fragment is not a scheme. Unknown schemes, file/drive/ADS forms,
absolute/UNC/device prefixes and protocol-relative requests never become file
capability targets. Targets remain untrusted values requiring native checks.

## Local resources and fragments

Split a literal fragment delimiter before decoding. Percent-decode path/fragment
once with strict UTF-8; keep original Unicode normalization. Relative parent/dot
segments are permitted as visible capability requests, not permission to escape
a root. Encoded path separators and encoded dot/dot-dot segments are denied.
Local query strings are outside this surface. Reject decoded colon/backslash/
question mark/absolute prefixes, trailing space/dot components (except literal
`.`/`..`) and reserved device basenames, including COM/LPT superscript variants,
trimmed device stems, and conservative CONIN$/CONOUT$ aliases.

The portable device guard follows the
[Windows naming rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file);
it intentionally also denies some POSIX-valid names. This is not a native
filesystem-equivalence or device-access proof. Other host-specific invalid
names can still require a clear platform error.

A target path containing a literal percent after decoding (for example a source
`literal%252e.txt` becoming `literal%2e.txt`) is a filename, not another URI.
**Adapters MUST NOT decode again, route it through a URL launcher, or expand
shell/tilde/environment syntax.** Pass this path to native file APIs under the
approved capability. Adapters must resolve relative to the actual document/base
identity, enforce root containment through symlinks/reparse points, reject native
device/absolute/invalid paths, handle revoked access, and require the expected
file/resource type before I/O. Plain `../` may be valid within an approved root;
lexical allowance never establishes containment.

Fragment-only links remain internal; fragment-only images are denied. Relative
images require the same capability path policy. Remote images are always denied,
including where an external HTTP link would be eligible. SVG/image decoding and
resource safety remain platform/core resource phases; this change adds no fetch.

## External activation

Optional url 2.5.8 provides WHATWG parsing and IDNA without I/O. HTTP(S) requires
an explicit exactly-two-slash authority prefix, a valid nonempty host and no
username/password. Missing/extra authority slashes are rejected instead of
silently repaired. International domains become ASCII IDNA and path spaces/
Unicode become the library's canonical encoding. Encoded control/format values
are denied before parsing; malformed hosts/IP literals/ports cannot activate.
No attempt is made to establish the remote endpoint's trustworthiness.

Mailto support is intentionally narrow: a nonempty ASCII address with one @,
nonempty ASCII letter/digit/dot/hyphen domain, no percent encoding, spaces,
query/header fields or fragment. It is not a complete RFC email-address parser;
unsupported email forms remain denied. Angle email autolinks get a semantic
mailto prefix while their source span remains the address lexeme.

Platform adapters must revalidate the scheme/target and require explicit user
activation under native policy; never concatenate document text into shell
commands or HTML. No core target triggers automatic network activity. CORE-003B4
retains deeper conformance/hostile tests and measured costs; production launch,
permissions, sandbox and native URL-handler behavior still need named-platform
implementation evidence.

## Dependencies and checks

Only feature `model-v2` enables url/IDNA/ICU; v1 remains the default. The shared
parser corrects v1 destination spans on blockquote continuation lines
(`> [a](\n> b)` now spans `b`, not `>`); `tests/parser.rs` pins this v1 change.
The
[full dependency inventory](Dependencies.md) records 60 all-feature packages,
reviewed Unicode notices and the footprint tradeoff. Parser/URL/provenance proof
work has no hard deadline guarantee; CORE-004A adds cooperative cancellation
between parser events and phases, not inside proof scans; B4 retains timing
gates. No new software installation is needed beyond normal Cargo dependency
fetching on the documented toolchain.

Regressions include Unicode/encoded names, spaces/fragments/parent-relative
requests, IDNs/mail, malformed/credential/scheme/encoding attacks, control/bidi
forms, device aliases, forged wire destinations/definition bindings, quote
continuations, linked images, budget independence and 8,192 generated v2 inputs.
They complement the frozen v1/model/parser corpus and other generated cases;
they are not exhaustive fuzzing or native filesystem validation.
