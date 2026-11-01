"""Rebuild a local source-only Graphify graph. Requires the optional graphifyy tool environment."""
from pathlib import Path
from collections import Counter
import json
import sys
from graphify.detect import detect, save_manifest
from graphify.extract import collect_files, extract
from graphify.build import build_from_json
from graphify.cluster import cluster, score_all
from graphify.analyze import god_nodes, surprising_connections, suggest_questions
from graphify.report import generate
from graphify.export import to_json
from graphify.diagnostics import diagnose_extraction

root = Path(__file__).resolve().parents[1]
out = root / 'graphify-out'
out.mkdir(exist_ok=True)
(out / '.graphify_python').write_text(sys.executable, encoding='utf-8')
(out / '.graphify_root').write_text(str(root), encoding='utf-8')
detection = detect(root / 'src')
files = collect_files(root / 'src')
print(f'Source-only corpus: {len(files)} files; docs/assets excluded.', flush=True)
data = extract(files, cache_root=root, root=root, parallel=False)
(out / '.graphify_extract.json').write_text(json.dumps(data, ensure_ascii=False), encoding='utf-8')
graph = build_from_json(data, root=str(root), directed=False)
if not graph.number_of_nodes():
    raise SystemExit('Empty AST graph; existing export preserved.')
communities = cluster(graph)
labels = {}
for key, nodes in communities.items():
    modules = Counter(Path(graph.nodes[n].get('source_file', 'external')).stem for n in nodes)
    labels[key] = ' / '.join(n for n, _ in modules.most_common(2))
if not to_json(graph, communities, str(out / 'graph.json'), community_labels=labels):
    raise SystemExit('Graph shrink guard: existing export preserved.')
report = generate(graph, communities, score_all(graph, communities), labels, god_nodes(graph), surprising_connections(graph, communities), detection, {'input': 0, 'output': 0}, str(root), suggested_questions=suggest_questions(graph, communities, labels))
(out / 'GRAPH_REPORT.md').write_text('Source-only deterministic AST graph; no semantic document extraction.\n\n' + report, encoding='utf-8')
health = diagnose_extraction(data, directed=False, root=str(root))
(out / 'health.json').write_text(json.dumps(health, ensure_ascii=False, default=str), encoding='utf-8')
(out / '.graphify_labels.json').write_text(json.dumps({str(k):v for k,v in labels.items()}), encoding='utf-8')
save_manifest({'code': [str(p) for p in files]}, root=str(root))
print(f'Graph: {graph.number_of_nodes()} nodes, {graph.number_of_edges()} edges; {len(communities)} groups.', flush=True)
print('Health:', {k:v for k,v in health.items() if 'endpoint' in k or 'collapsed' in k or 'loop' in k})
