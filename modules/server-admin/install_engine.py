#!/usr/bin/env python3
"""Install only the administration engine and its adapter into an explicit game root."""
import argparse, json, shutil, os, hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('root',type=Path);p.add_argument('--emf',type=Path,default=Path.home()/'Downloads/EMF-0.3.3-manual');a=p.parse_args()
repo=Path(__file__).resolve().parents[2];root=a.root.resolve()
if not (root/'enshrouded_server.exe').is_file():p.error('Expected a dedicated server directory')
for source in (a.emf/'game/dinput8.dll',a.emf/'game/emf/emf_core.dll',repo/'build/server-admin/server-admin-engine.dll'):
 if not source.is_file():p.error('Missing build input: '+str(source))
existing=root/'emf/mods'
if existing.is_dir() and any(child.name!='server-admin' for child in existing.iterdir()):p.error('Existing EMF gameplay mods found. Refusing to enable them implicitly.')
pins=json.loads((Path(__file__).parent/'engine-dependencies.json').read_text())['files']
for relative in ('game/dinput8.dll','game/emf/emf_core.dll'):
 if hashlib.sha256((a.emf/relative).read_bytes()).hexdigest()!=pins[relative]:p.error('Engine dependency checksum mismatch: '+relative)
package=root/'mods/admin-engine';(package/'src').mkdir(parents=True,exist_ok=True)
(package/'mod.json').write_text(json.dumps({'id':'admin-engine','name':'Server Administration Engine','version':'0.1.0','description':'Native transactions for the local dashboard. No automatic gameplay modifiers.','authors':[],'capabilities':['runtime','runtime-register-dll']},indent=2))
(package/'extended.mod.json').write_text(json.dumps({'schemaVersion':1,'enabled':True,'targets':['server']},indent=2))
(package/'src/mod.lua').write_text('-- Engine bootstrap only. Commands require the local dashboard.\n')
(package/'native-plugin.ini').write_text('[Plugin]\nEnabled=1\nDll=engine-loader.dll\n')
shutil.copy2(a.emf/'game/dinput8.dll',package/'engine-loader.dll')
engine=root/'emf';(engine/'mods/server-admin').mkdir(parents=True,exist_ok=True)
shutil.copy2(a.emf/'game/emf/emf_core.dll',engine/'emf_core.dll')
shutil.copy2(a.emf/'LICENSE.txt',engine/'LICENSE.txt')
shutil.copy2(repo/'build/server-admin/server-admin-engine.dll',engine/'mods/server-admin/server-admin.dll')
(engine/'mods/server-admin/mod.json').write_text(json.dumps({'id':'server-admin','name':'Local Server Administration','version':'0.1.0','emf':21,'enabled':True,'description':'Explicit dashboard operations only; no gameplay modifiers.','settings':[]},indent=2))
if not (engine/'emf_core.ini').exists():(engine/'emf_core.ini').write_text('[debug]\nbridge=0\n[chat]\nallow_commands=0\n')
for path in [*package.rglob('*'),*engine.rglob('*')]:
 if path.is_file():path.chmod(path.stat().st_mode|0o200)
print('Installed administration adapter into',root)
