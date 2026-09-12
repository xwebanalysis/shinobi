import sys
from pathlib import Path

EXTRACTOR_ROOT = Path(__file__).resolve().parent.parent
if str(EXTRACTOR_ROOT) not in sys.path:
    sys.path.insert(0, str(EXTRACTOR_ROOT))
