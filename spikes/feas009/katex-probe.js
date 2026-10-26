// Original synthetic cases; no copied user documents.
JSON.stringify((() => {
  const cases = [
    ["inline", "a^2+b^2=c^2"],
    ["display", String.raw`\sum_{i=1}^{n}i=\frac{n(n+1)}2`],
    ["integral", String.raw`\int_0^1 x\,dx`],
    ["invalid", String.raw`\frac{1}{`],
    ["unsafe-link", String.raw`\href{javascript:alert(1)}{unsafe}`],
    ["remote-image", String.raw`\includegraphics{https://example.invalid/probe.png}`],
    ["macro-loop", String.raw`\def\a{\a}\a`]
  ];
  const results = [];
  for (const [name, source] of cases) {
    const samples = [];
    let output = "", error = null;
    for (let i = 0; i < 6; i++) {
      const t = clockNow();
      try {
        output = katex.renderToString(source, {
          displayMode: name === "display", trust: false, strict: "error",
          maxExpand: 100, maxSize: 20, throwOnError: true, output: "htmlAndMathml"
        });
      } catch (e) { error = String(e); }
      if (i) samples.push(clockNow() - t);
    }
    results.push({name, samples_ms: samples, error, output_bytes: unescape(encodeURIComponent(output)).length,
      has_javascript_href: /href\s*=\s*["']javascript:/i.test(output),
      has_remote_image: /<img\b[^>]*src\s*=\s*["']https?:/i.test(output),
      has_mathml: output.includes("<math"), has_html: output.includes('class="katex')});
  }
  return {engine: "katex-javascriptcore", clock: "native monotonic clock", results};
})())
