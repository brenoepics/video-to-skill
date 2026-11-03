"""v2s-bench: does a video-derived skill make an agent better at real tasks?

The harness is deliberately dependency-light. Every published figure is
recomputed from immutable run records by pure-Python code in
``v2sbench.stats``, so a reader can audit the arithmetic without
installing a scientific stack.
"""

__version__ = "0.1.0"

SCHEMA_VERSION = 1
