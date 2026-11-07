#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Fail if upstream changes the exact unversioned bootstrap signatures Kiln supports."""
import pathlib
import re
root = pathlib.Path(__file__).resolve().parents[1]
src = (root / 'modules/entrypoints/loader/embedded.rs').read_text()
for symbol, args, result in [
    ('shroudforge_prepare_startup', 'game: *const u16', 'bool'),
    ('shroudforge_create', 'game: *const u16', '*mut RuntimeHandle'),
    ('shroudforge_update', 'handle: *mut RuntimeHandle, delta_seconds: f64,', 'bool'),
    ('shroudforge_destroy', 'handle: *mut RuntimeHandle', None),
]:
    suffix = r'\s*\{' if result is None else r'\s*->\s*' + re.escape(result) + r'\s*\{'
    pattern = re.escape('#[unsafe(no_mangle)]') + r'\s*pub unsafe extern "C" fn ' + symbol + r'\(\s*' + re.escape(args).replace(r'\ ', r'\s*') + r'\s*\)' + suffix
    if not re.search(pattern, src):
        raise SystemExit('bootstrap signature changed: ' + symbol)
