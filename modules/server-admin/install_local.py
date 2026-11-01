#!/usr/bin/env python3
"""Install the tested administration build on an idle local dedicated server, preserving rollback files."""
import argparse,importlib.util,json,os,shutil,signal,subprocess,time,hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--root',type=Path,default=Path.home()/'.local/share/Steam/steamapps/common/EnshroudedServer');a=p.parse_args();root=a.root.resolve();module=Path(__file__).resolve().parent;repo=module.parents[1]
s=importlib.util.spec_from_file_location('dashboard',module/'dashboard.py');d=importlib.util.module_from_spec(s);s.loader.exec_module(d)
config=d.read_json(root/'enshrouded_server.json',{});status=d.query_server(config.get('queryPort',15637))
if status.get('online') and status.get('players')!=0:p.error('Players are connected. Refusing an automatic restart.')
new=repo/'target/x86_64-pc-windows-msvc/release/shroudforge_modloader.dll'
if not new.is_file() or not (repo/'build/server-admin/server-admin-engine.dll').is_file():p.error('Build the runtime and engine first')
existing=root/'emf/mods'
if existing.is_dir() and any(child.name!='server-admin' for child in existing.iterdir()):p.error('Existing EMF gameplay packages require manual migration.')
pids=[]
for proc in Path('/proc').iterdir():
 if not proc.name.isdigit():continue
 try:
  if (proc/'comm').read_text().strip()=='enshrouded_serv' and (proc/'cwd').resolve()==root:pids.append(int(proc.name))
 except OSError:pass
if status.get('online') and len(pids)!=1:p.error('Could not unambiguously identify the running server')
for pid in pids:os.kill(pid,signal.SIGINT)
end=time.monotonic()+40
while any(Path('/proc',str(pid)).exists() for pid in pids):
 if time.monotonic()>end:p.error('Server did not stop cleanly; files were not replaced')
 time.sleep(.2)
backup=Path.home()/'.local/state/shroudforge-admin/deployment-backups'/time.strftime('%Y%m%d-%H%M%S');backup.mkdir(parents=True,mode=0o700)
for relative in ('shroudforge/shroudforge-runtime.dll','enshrouded_server.json','shroudforge/config/modloader-config.json'):
 source=root/relative;dest=backup/relative;dest.parent.mkdir(parents=True,exist_ok=True)
 if source.exists():shutil.copy2(source,dest)
for relative in ('savegame','mods/admin-engine','emf'):
 source=root/relative
 if source.exists():shutil.copytree(source,backup/relative,symlinks=True)
try:
 subprocess.run(['python3',str(module/'install_engine.py'),str(root)],check=True)
 shutil.copy2(new,root/'shroudforge/shroudforge-runtime.dll')
 manifest={'root':str(root),'backup':str(backup),'runtimeSha256':hashlib.sha256(new.read_bytes()).hexdigest(),'installedAt':time.time()}
 (backup/'deployment.json').write_text(json.dumps(manifest,indent=2))
except Exception:
 shutil.copy2(backup/'shroudforge/shroudforge-runtime.dll',root/'shroudforge/shroudforge-runtime.dll');raise
app=Path.home()/'.local/share/shroudforge-admin';app.mkdir(parents=True,exist_ok=True)
shutil.copy2(module/'dashboard.py',app/'dashboard.py');shutil.copytree(module/'web',app/'web',dirs_exist_ok=True);shutil.copy2(module/'README.md',app/'README.md')
units=Path.home()/'.config/systemd/user';units.mkdir(parents=True,exist_ok=True)
unit=units/'enshrouded-admin.service';unit.write_text(f'''[Unit]
Description=Emberwatch local Enshrouded administration
After=default.target
[Service]
ExecStart=/usr/bin/python3 {app}/dashboard.py --root {root}
Restart=on-failure
RestartSec=3
NoNewPrivileges=true
UMask=0077
[Install]
WantedBy=default.target
''')
service_env=os.environ.copy();user_runtime=Path('/run/user')/str(os.getuid())
if (user_runtime/'bus').exists():
 service_env.setdefault('XDG_RUNTIME_DIR',str(user_runtime));service_env.setdefault('DBUS_SESSION_BUS_ADDRESS','unix:path='+str(user_runtime/'bus'))
subprocess.run(['systemctl','--user','daemon-reload'],check=True,env=service_env);subprocess.run(['systemctl','--user','enable','--now','enshrouded-admin.service'],check=True,env=service_env)
apps=Path.home()/'.local/share/applications';apps.mkdir(parents=True,exist_ok=True)
(apps/'enshrouded-admin.desktop').write_text('[Desktop Entry]\nType=Application\nName=Emberwatch Server Dashboard\nComment=Local Enshrouded administration\nExec=xdg-open http://127.0.0.1:8787\nIcon=applications-games\nTerminal=false\nCategories=Game;Utility;\n')
subprocess.run(['steam','steam://rungameid/2278520'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=15)
print('Installed. Dashboard: http://127.0.0.1:8787\nRollback backup:',backup,flush=True)
end=time.monotonic()+60
while time.monotonic()<end:
 state=d.read_json(root/'shroudforge/runtime/admin/engine/state.json',{})
 if time.time()-state.get('updatedAt',0)<5 and d.query_server(config.get('queryPort',15637)).get('online'):
  print('Server online; native administration heartbeat is live.',flush=True);break
 time.sleep(1)
else:raise SystemExit('Server startup has not been confirmed. Inspect its logs and the rollback backup.')
