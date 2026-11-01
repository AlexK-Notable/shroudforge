"""Deploy the build ONLY into the existing isolated test copy and restart that copy."""
from pathlib import Path
import os,subprocess,signal,time,shutil
root=Path('/home/jame/Work/sf-winefix/server');repo=Path(__file__).resolve().parents[2]
matched=[]
for p in Path('/proc').iterdir():
 if not p.name.isdigit():continue
 try:
  if (p/'comm').read_text().strip()=='enshrouded_serv' and (p/'cwd').resolve()==root:matched.append(int(p.name))
 except OSError:pass
for pid in matched:os.kill(pid,signal.SIGINT)
end=time.monotonic()+30
while any(Path('/proc',str(pid)).exists() for pid in matched):
 if time.monotonic()>end:raise SystemExit('Test server did not stop cleanly; not replacing its DLLs')
 time.sleep(.2)
shutil.copy2(repo/'target/x86_64-pc-windows-msvc/release/shroudforge_modloader.dll',root/'shroudforge/shroudforge-runtime.dll')
subprocess.run(['python3',str(repo/'modules/server-admin/install_engine.py'),str(root)],check=True)
env=os.environ.copy();env.update(STEAM_COMPAT_DATA_PATH='/home/jame/Work/sf-winefix/pfx',STEAM_COMPAT_CLIENT_INSTALL_PATH='/home/jame/.local/share/Steam',SteamAppId='2278520',WINEDLLOVERRIDES='dinput8,dbghelp,winmm=n,b')
with open('/home/jame/Work/sf-winefix/out/admin-test-latest.log','w') as f:
 p=subprocess.Popen(['/home/jame/.local/share/Steam/compatibilitytools.d/GE-Proton11-11-x86_64/proton','run','./enshrouded_server.exe'],cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
print('Test server wrapper:',p.pid)
