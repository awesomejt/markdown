mermaid.initialize({startOnLoad: false, securityLevel: 'strict', htmlLabels: false,
  flowchart: {htmlLabels: false}, maxTextSize: 10000, maxEdges: 100});
const cases = [
  ['flowchart', 'graph TD\n A[Start] --> B[Done]'],
  ['sequence', 'sequenceDiagram\n Alice->>Bob: Hello\n Bob-->>Alice: Reply'],
  ['invalid', 'graph TD\n A --> --> (('],
  ['unsafe-link', 'graph TD\n A[Unsafe]\n click A "javascript:alert(1)"'],
  ['html-label', 'graph TD\n A["<img src=https://example.invalid/probe.png onerror=alert(1)>"] --> B']
];
const results = [];
let sequence = 0;
for (const [name, source] of cases) {
  const samples = [];
  let svg = '', error = null;
  for (let i=0; i<6; i++) {
    const t=performance.now();
    try {
      const rendered=await mermaid.render('probe'+sequence++, source);
      svg=rendered.svg;
    } catch(e) { error=String(e); }
    if(i) samples.push(performance.now()-t);
  }
  const parsed=new DOMParser().parseFromString(svg, 'image/svg+xml');
  const root=parsed.documentElement;
  results.push({name, samples_ms:samples, error, output_bytes:new TextEncoder().encode(svg).length,
    svg_valid:root.localName==='svg' && !parsed.querySelector('parsererror'),
    scripts:parsed.querySelectorAll('script').length,
    foreign_objects:parsed.querySelectorAll('foreignObject').length,
    unsafe_attributes:Array.from(parsed.querySelectorAll('*')).flatMap(e=>Array.from(e.attributes))
      .filter(a=>/^on/i.test(a.name) ||
        (/^(href|xlink:href|src)$/i.test(a.name) && /^(javascript:|https?:)/i.test(a.value)))
      .map(a=>[a.name,a.value])});
  document.querySelector('#output').textContent='';
}
return {engine:'mermaid-wkwebview', clock:'performance.now', results};
