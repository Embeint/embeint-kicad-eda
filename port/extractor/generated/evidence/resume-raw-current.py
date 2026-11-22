#!/usr/bin/env python3
# Historical raw run: retain its exact cache identity, guard proven deletions.
import gzip
import json
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent / 'raw-runner/port'))
from tools import extract_graph
from extractor.clang_graph import extract_translation_unit

class CachePath(type(Path())):
    def is_raw_cache(self):
        return self.parent == Path.home() / 'kicad-port/extract-cache' and self.suffix == '.json'

    def exists(self, **kwargs):
        return super().exists(**kwargs) or (self.is_raw_cache() and self.with_suffix('.json.gz').exists())

    def read_text(self, *args, **kwargs):
        if self.is_raw_cache() and not super().exists():
            return gzip.decompress(self.with_suffix('.json.gz').read_bytes()).decode('utf-8')
        return super().read_text(*args, **kwargs)


def guarded_extract(job):
    source = Path(job['file'].replace(job['old_root'], job['root']))
    if not source.is_file():
        record = json.loads((Path.home() / 'kicad-port/reconciliation/reconciliation.json').read_text())
        relative = str(source).removeprefix(job['root'] + '/')
        if relative in record['stale_deleted_sources']:
            return {'file': relative, 'status': 'stale-excluded', 'functions': {},
                    'calls': [], 'overrides': [], 'unresolved': [],
                    'diagnostics': [{'severity': 0, 'message': 'Source proven deleted by pinned manifest and CMake reconciliation; not parsed'}]}
        raise FileNotFoundError(source)
    return extract_translation_unit(job)


if __name__ == '__main__':
    extract_graph.Path = CachePath
    extract_graph.extract_translation_unit = guarded_extract
    sys.exit(extract_graph.main())