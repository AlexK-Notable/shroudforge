import importlib.util,json,tempfile,time,unittest,threading,urllib.request,urllib.error
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('dashboard',Path(__file__).resolve().parents[1]/'dashboard.py');d=importlib.util.module_from_spec(spec);spec.loader.exec_module(d)
class AdministrationTests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name)/'server';self.root.mkdir();self.env=patch.dict(d.os.environ,{'XDG_STATE_HOME':self.tmp.name});self.env.start();self.a=d.Administration(self.root)
  self.p={'entity':12,'account':'123','inventory':[{'item':55,'count':3,'inventory':22,'slot':0}]}
  self.snap={'engineLive':True,'shroudforgeLive':True,'engine':{'session':'test','players':[self.p],'settings':[{'key':'loot','min':.25,'max':2}]},'shroudforge':{'pid':10}}
  self.a.snapshot=lambda:self.snap
 def tearDown(self):self.env.stop();self.tmp.cleanup()
 def test_stale_session_and_inventory_never_reach_queue(self):
  for r in [{'op':'save','session':'old'},{'op':'inventory.remove','session':'test','entity':12,'account':'123','expectedInventory':[],'count':1}]:
   with self.assertRaises(d.AdminError):self.a.submit('engine',r)
  self.assertFalse((self.a.ipc/'engine/requests').exists())
 def test_mutation_is_correlated_and_preserves_inventory_preimage(self):
  r=self.a.submit('engine',{'op':'inventory.remove','session':'test','entity':12,'account':'123','expectedInventory':self.p['inventory'],'count':1,'item':55,'inventory':22,'slot':0})
  self.assertTrue((self.a.data/'inventory-before'/(r['id']+'.json')).is_file())
  packet=d.read_json(self.a.ipc/'engine/requests'/(r['id']+'.json'));self.assertEqual(packet['id'],r['id']);self.assertLess(abs(packet['createdAt']-time.time()),1)
 def test_out_of_range_or_nonfinite_setting_rejected(self):
  for value in [3,float('nan'),True,'1']:
   with self.assertRaises(d.AdminError):self.a.submit('engine',{'op':'settings.set','session':'test','key':'loot','value':value})
 def test_unknown_commands_and_path_traversal_rejected(self):
  with self.assertRaises(d.AdminError):self.a.submit('engine',{'op':'exec','session':'test'})
  with self.assertRaises(d.AdminError):self.a.result('engine','../secret')
 def test_pending_result_is_not_reported_success(self):
  r=self.a.result('engine','a'*32);self.assertNotIn('ok',r);self.assertEqual(r['status'],'pending')
 def test_failed_transaction_result_is_preserved_in_audit(self):
  id='b'*32;r={'id':id,'ok':False,'data':{'partial':True,'completed':2,'requested':3}}
  d.atomic_json(self.a.ipc/'engine/results'/(id+'.json'),r)
  self.assertFalse(self.a.result('engine',id)['ok']);self.assertEqual(self.a.audit_tail()[0]['status'],'failed')
 def test_http_denies_rebinding_and_cross_origin_writes(self):
  srv=d.ThreadingHTTPServer(('127.0.0.1',0),d.Handler);srv.admin=self.a
  t=threading.Thread(target=srv.serve_forever,daemon=True);t.start();base=f'http://127.0.0.1:{srv.server_port}'
  try:
   for headers in [{'Host':'evil.example'},{'Origin':'https://evil.example','X-Admin-Token':self.a.token}]:
    req=urllib.request.Request(base+'/api/commands',b'{}',headers={'Content-Type':'application/json',**headers})
    with self.assertRaises(urllib.error.HTTPError) as e:urllib.request.urlopen(req)
    self.assertEqual(e.exception.code,403);e.exception.close()
  finally:srv.shutdown();srv.server_close();t.join()
if __name__=='__main__':unittest.main()
