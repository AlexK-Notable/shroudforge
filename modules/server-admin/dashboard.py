#!/usr/bin/env python3
"""Loopback-only ShroudForge server administration. Python standard library; no cloud service."""
from __future__ import annotations
import argparse, hashlib, hmac, json, math, os, re, secrets, shutil, socket, struct, tarfile, threading, time, uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

class AdminError(Exception):
    def __init__(self,message,status=400): super().__init__(message);self.status=status

def read_json(path,default=None):
    try:return json.loads(Path(path).read_text())
    except (OSError,ValueError):return default

def atomic_json(path,value):
    path=Path(path);path.parent.mkdir(parents=True,exist_ok=True)
    tmp=path.with_name(path.name+'.'+uuid.uuid4().hex+'.tmp')
    try:
        with tmp.open('x') as f:json.dump(value,f,ensure_ascii=False,allow_nan=False);f.flush();os.fsync(f.fileno())
        os.replace(tmp,path)
    finally:tmp.unlink(missing_ok=True)

def query_server(port):
    s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);s.settimeout(0.7)
    try:
        request=b'\xff'*4+b'TSource Engine Query\x00';s.sendto(request,('127.0.0.1',port));data,_=s.recvfrom(65535)
        if data[4:5]==b'A':s.sendto(request+data[5:9],('127.0.0.1',port));data,_=s.recvfrom(65535)
        if data[:5]!=b'\xff'*4+b'I':raise ValueError('Unexpected query response')
        off=6;out={'online':True}
        for k in ('name','map','folder','game'):
            end=data.index(b'\0',off);out[k]=data[off:end].decode(errors='replace');off=end+1
        off+=2;out.update(players=data[off],capacity=data[off+1],passwordProtected=bool(data[off+5]));return out
    except (OSError,ValueError,IndexError):return {'online':False}
    finally:s.close()

ENGINE_OPS={'ping','inventory.add','inventory.remove','teleport','teleport.grave','settings.set','settings.reset','save','items'}
SF_OPS={'ping','reload','unload','mod.enabled','mod.settings'}

class Administration:
    def __init__(self,root):
        self.root=Path(root).resolve();self.runtime=self.root/'shroudforge/runtime';self.ipc=self.runtime/'admin'
        self.lock=threading.RLock();self.token=secrets.token_urlsafe(32);self.query_cache=(0,{})
        self.data=Path(os.environ.get('XDG_STATE_HOME',Path.home()/'.local/state'))/'shroudforge-admin'/hashlib.sha256(str(self.root).encode()).hexdigest()[:12]
        self.data.mkdir(parents=True,exist_ok=True);os.chmod(self.data,0o700)
    def snapshot(self):
        config=read_json(self.root/'enshrouded_server.json',{})
        with self.lock:
            if time.monotonic()-self.query_cache[0]>2:self.query_cache=(time.monotonic(),query_server(int(config.get('queryPort',15637))))
            query=self.query_cache[1]
        sf=read_json(self.ipc/'shroudforge/state.json',{})
        engine=read_json(self.ipc/'engine/state.json',{})
        runtime=read_json(self.root/'shroudforge/state.json',{}).get('runtime',{})
        sf_fresh=time.time()-sf.get('updatedAt',0)<6 and query.get('online',False)
        engine_fresh=time.time()-engine.get('updatedAt',0)<6 and query.get('online',False)
        return {'server':query,'root':str(self.root),'queryPort':config.get('queryPort',15637),'shroudforge':sf,'runtime':runtime,'engine':engine,'shroudforgeLive':sf_fresh,'engineLive':engine_fresh,'audit':self.audit_tail(),'backups':self.backups()}
    def audit_tail(self):
        p=self.data/'audit.jsonl'
        if not p.exists():return []
        with p.open('rb') as f:f.seek(max(0,p.stat().st_size-60000));lines=f.read().splitlines()
        out=[]
        for line in lines[-40:]:
            try:out.append(json.loads(line))
            except ValueError:pass
        return out[::-1]
    def audit(self,event):
        with self.lock:
            with (self.data/'audit.jsonl').open('a') as f:f.write(json.dumps({'at':time.time(),**event},allow_nan=False)+'\n')
    def backups(self):
        p=self.data/'backups'
        return [{'name':f.name,'size':f.stat().st_size,'at':f.stat().st_mtime} for f in sorted(p.glob('*.tar.gz'),reverse=True)[:12]] if p.exists() else []
    def backup(self):
        # Force-save acknowledgement first; archive copies of the completed save files.
        snap=self.snapshot()
        if snap['server'].get('online'):
            if not snap['engineLive']:raise AdminError('The live engine bridge is required to save before backup.',409)
            response=self.wait(self.submit('engine',{'op':'save','session':snap['engine']['session']}),20)
            if not response.get('ok'):raise AdminError('Save was not confirmed. Backup was not started.',409)
        config=read_json(self.root/'enshrouded_server.json',{});save=Path(config.get('saveDirectory','./savegame'))
        if not save.is_absolute():save=self.root/save
        if not save.is_dir():raise AdminError('Save directory does not exist.')
        dest=self.data/'backups';dest.mkdir(exist_ok=True);name=time.strftime('%Y%m%d-%H%M%S')+'-'+uuid.uuid4().hex[:6]+'.tar.gz'
        staging=self.data/('save-copy-'+uuid.uuid4().hex)
        try:
            def generation():
                return {str(p.relative_to(save)):(p.stat().st_size,p.stat().st_mtime_ns) for p in save.rglob('*') if p.is_file() and not p.is_symlink()}
            before=generation()
            if any(p.is_symlink() for p in save.rglob('*')):raise AdminError('Save directory contains symlinks; backup refused.')
            shutil.copytree(save,staging,symlinks=False)
            if generation()!=before:raise AdminError('Save files changed during copying. Retry the backup.',409)
            with tarfile.open(dest/name,'w:gz') as archive:archive.add(staging,arcname='savegame',recursive=True)
        finally:shutil.rmtree(staging,ignore_errors=True)
        self.audit({'operation':'backup','name':name,'status':'completed'})
        return {'ok':True,'name':name,'size':(dest/name).stat().st_size}
    def submit(self,channel,payload):
        if channel not in ('engine','shroudforge'):raise AdminError('Unknown command channel.')
        allowed=ENGINE_OPS if channel=='engine' else SF_OPS
        if not isinstance(payload,dict) or payload.get('op') not in allowed:raise AdminError('Unsupported command.')
        snap=self.snapshot()
        if not snap['engineLive' if channel=='engine' else 'shroudforgeLive']:raise AdminError('Runtime bridge is offline or stale. Start the server with the administration build.',409)
        request=dict(payload);request['id']=uuid.uuid4().hex;request['createdAt']=time.time()
        if channel=='shroudforge':request['pid']=snap['shroudforge']['pid']
        else:
            if request.get('session')!=snap['engine'].get('session'):raise AdminError('Session changed. Refresh before retrying.',409)
            if request['op'].startswith('inventory.') or request['op'].startswith('teleport'):
                match=next((p for p in snap['engine'].get('players',[]) if p['entity']==request.get('entity') and p['account']==request.get('account')),None)
                if match is None:raise AdminError('Player disconnected. Refresh before retrying.',409)
                if request['op'].startswith('inventory.'):
                    if request.get('expectedInventory')!=match.get('inventory'):raise AdminError('Inventory changed. Refresh before retrying.',409)
                    count=request.get('count')
                    if type(count) is not int or not 1<=count<=65535:raise AdminError('Count must be an integer from 1 to 65535.')
            if request['op']=='settings.set':
                setting=next((s for s in snap['engine'].get('settings',[]) if s['key']==request.get('key')),None);value=request.get('value')
                if not setting or type(value) not in (int,float) or not math.isfinite(value) or not setting['min']<=value<=setting['max']:raise AdminError('Setting outside the supported range.')
        directory=self.ipc/channel/'requests';directory.mkdir(parents=True,exist_ok=True)
        if sum(1 for _ in directory.iterdir())>=64:raise AdminError('Command queue is full.',429)
        if len(json.dumps(request).encode())>60000:raise AdminError('Command too large.',413)
        if request['op'].startswith('inventory.'):
            atomic_json(self.data/'inventory-before'/(request['id']+'.json'),{k:request.get(k) for k in ('id','session','account','entity','expectedInventory','createdAt')})
        atomic_json(directory/(request['id']+'.json'),request)
        self.audit({'id':request['id'],'channel':channel,'operation':request['op'],'account':request.get('account'),'status':'queued'})
        return {'id':request['id'],'channel':channel,'status':'queued'}
    def result(self,channel,id):
        if channel not in ('engine','shroudforge') or not re.fullmatch('[0-9a-f]{32}',id):raise AdminError('Invalid request ID.')
        p=self.ipc/channel/'results'/(id+'.json');r=read_json(p)
        if r is not None:
            marker=self.data/'results'/(id+'.json')
            with self.lock:
                if not marker.exists():atomic_json(marker,r);self.audit({'id':id,'channel':channel,'status':'completed' if r.get('ok') else 'failed','result':r})
            return r
        for suffix in ('.processing','.json.processing'):
            if (self.ipc/channel/'requests'/(id+suffix)).exists():return {'id':id,'status':'processing'}
        return {'id':id,'status':'pending','message':'Awaiting a result. A timeout does not mean the action was cancelled; do not automatically retry.'}
    def wait(self,request,seconds=15):
        end=time.monotonic()+seconds
        while time.monotonic()<end:
            r=self.result(request['channel'],request['id'])
            if 'ok' in r:return r
            time.sleep(.2)
        return {'ok':False,'uncertain':True}

class Handler(BaseHTTPRequestHandler):
    server_version='ShroudForgeAdmin/0.1'
    def log_message(self,*args):pass
    @property
    def admin(self):return self.server.admin
    def respond(self,status,data,kind='application/json'):
        raw=json.dumps(data,ensure_ascii=False,allow_nan=False).encode() if kind=='application/json' else data
        self.send_response(status);self.send_header('Content-Type',kind);self.send_header('Content-Length',str(len(raw)))
        self.send_header('Cache-Control','no-store');self.send_header('X-Content-Type-Options','nosniff');self.send_header('Referrer-Policy','no-referrer')
        self.send_header('Content-Security-Policy',"default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'self'")
        self.end_headers();self.wfile.write(raw)
    def guard(self,write=False):
        hosts={f'127.0.0.1:{self.server.server_port}',f'localhost:{self.server.server_port}'}
        if self.headers.get('Host') not in hosts:raise AdminError('Invalid local host.',403)
        origin=self.headers.get('Origin');expected='http://'+self.headers.get('Host','')
        if origin and origin!=expected:raise AdminError('Cross-origin access denied.',403)
        if self.headers.get('Sec-Fetch-Site')=='cross-site':raise AdminError('Cross-site access denied.',403)
        if write:
            if origin!=expected:raise AdminError('A same-origin request is required.',403)
            if not hmac.compare_digest(self.headers.get('X-Admin-Token',''),self.admin.token):raise AdminError('Invalid administration token.',403)
            if self.headers.get_content_type()!='application/json':raise AdminError('Expected JSON.',415)
    def do_GET(self):
        try:
            self.guard();p=urlsplit(self.path).path
            if p=='/api/session':return self.respond(200,{'token':self.admin.token})
            if p=='/api/status':return self.respond(200,self.admin.snapshot())
            if p=='/api/items':return self.respond(200,read_json(self.admin.ipc/'engine/items.json',[]))
            if p.startswith('/api/results/'):
                parts=p.split('/');return self.respond(200,self.admin.result(parts[-2],parts[-1]))
            if p=='/api/logs':
                out={}
                for key,path in {'server':'logs/enshrouded_server.log','shroudforge':'shroudforge/logs/shroudforge-server.log','engine':'emf/emf_core.log'}.items():
                    f=self.admin.root/path
                    try:
                        with f.open('rb') as stream:stream.seek(max(0,f.stat().st_size-30000));out[key]=stream.read().decode(errors='replace')
                    except OSError:out[key]=''
                return self.respond(200,out)
            names={'/':'index.html','/app.js':'app.js','/style.css':'style.css'}
            if p not in names:raise AdminError('Not found.',404)
            file=Path(__file__).parent/'web'/names[p];kind={'.html':'text/html; charset=utf-8','.js':'text/javascript; charset=utf-8','.css':'text/css; charset=utf-8'}[file.suffix]
            self.respond(200,file.read_bytes(),kind)
        except AdminError as e:self.respond(e.status,{'error':str(e)})
        except Exception:self.respond(500,{'error':'Failed to read local server state.'})
    def do_POST(self):
        try:
            self.guard(True);length=int(self.headers.get('Content-Length','0'))
            if not 0<length<=65536:raise AdminError('Request size outside allowed range.',413)
            body=json.loads(self.rfile.read(length));p=urlsplit(self.path).path
            if p=='/api/commands':r=self.admin.submit(body.get('channel'),body.get('request'));return self.respond(202,r)
            if p=='/api/backup':return self.respond(200,self.admin.backup())
            raise AdminError('Not found.',404)
        except AdminError as e:self.respond(e.status,{'error':str(e)})
        except (ValueError,TypeError,AttributeError):self.respond(400,{'error':'Malformed JSON request.'})
        except Exception:self.respond(500,{'error':'Local administration operation failed.'})

def main():
    p=argparse.ArgumentParser();p.add_argument('--root',type=Path,default=Path.home()/'.local/share/Steam/steamapps/common/EnshroudedServer');p.add_argument('--port',type=int,default=8787);a=p.parse_args()
    if not (a.root/'enshrouded_server.exe').is_file():p.error('Dedicated server executable not found')
    server=ThreadingHTTPServer(('127.0.0.1',a.port),Handler);server.admin=Administration(a.root)
    print(f'ShroudForge administration: http://127.0.0.1:{a.port}',flush=True)
    try:server.serve_forever()
    except KeyboardInterrupt:pass
    finally:server.server_close()
if __name__=='__main__':main()
