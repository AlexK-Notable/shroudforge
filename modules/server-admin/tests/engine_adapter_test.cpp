#include "../native/engine_adapter.cpp"
#include <cassert>
#include <iostream>
namespace {
gen::PlayerState fake_player{};
gen::CurrentTransform fake_transform{};
gen::InventorySetup fake_setup{};
gen::Inventory fake_inventory{};
std::vector<uint8_t> fake_infos(1023 * 0xa0);
int mutations = 0;
bool partial = false;
double fake_setting = 1.0;
void *component(EmfWorld *, uint32_t hash, uint32_t e, uint64_t *size) {
  void *p = nullptr;
  uint64_t n = 0;
  if (e == 1 && hash == gen::PlayerState::kHash) {
    p = &fake_player;
    n = sizeof fake_player;
  }
  if (e == 1 && hash == gen::CurrentTransform::kHash) {
    p = &fake_transform;
    n = sizeof fake_transform;
  }
  if (e == 1 && hash == gen::InventorySetup::kHash) {
    p = &fake_setup;
    n = sizeof fake_setup;
  }
  if (e == 21 && hash == gen::Inventory::kHash) {
    p = &fake_inventory;
    n = sizeof fake_inventory;
  }
  if (size)
    *size = n;
  return p;
}
J request(const std::string &op) {
  return {{"op", op},
          {"session", session},
          {"entity", 1},
          {"account", "1234"},
          {"expectedInventory",
           stacks(emf::World(reinterpret_cast<EmfWorld *>(1)), 1)},
          {"count", 1},
          {"item", 55},
          {"inventory", 21},
          {"slot", 0}};
}
} // namespace
int main() {
  EmfApi a{};
  a.version = EMF_API_VERSION;
  a.size = sizeof a;
  a.get_component = component;
  a.singleton = [](EmfWorld *, const char *) -> void * {
    return fake_infos.data();
  };
  a.item_info = [](EmfWorld *, uint32_t id, EmfItemInfo *i) {
    if (id != 55)
      return false;
    i->item_id = 55;
    i->max_stack = 100;
    strcpy_s(i->name, "Wood");
    return true;
  };
  a.put_items_ex = [](EmfWorld *w, const EmfItemsRequest *r) {
    mutations++;
    uint32_t n = partial ? r->count - 1 : r->count;
    fake_inventory.slots[0].data.count += n;
    r->cb(w, r->entity, r->item, r->count, n, r->user);
    return true;
  };
  a.take_items = [](EmfWorld *w, const EmfItemsRequest *r) {
    assert(r->flags & EMF_ITEMS_EXACT_SLOT);
    mutations++;
    fake_inventory.slots[0].data.count -= r->count;
    r->cb(w, r->entity, r->item, r->count, r->count, r->user);
    return true;
  };
  a.world_setting_get = [](const char *, double *v) {
    *v = fake_setting;
    return true;
  };
  a.world_setting_set = [](const char *, double, uint32_t) { return true; };
  a.world_kind = [](EmfWorld *) -> uint8_t { return EMF_WORLD_SERVER; };
  a.world_tick = [](EmfWorld *) -> uint32_t { return 100; };
  emf::api_ref() = &a;
  session = "unit-test";
  ipc = fs::temp_directory_path() /
        ("sf-admin-tests-" + std::to_string(GetCurrentProcessId()));
  fs::create_directories(ipc / "results");
  uint32_t held = 1;
  uint64_t acct = 1234;
  memcpy(fake_infos.data(), &held, 4);
  memcpy(fake_infos.data() + 8, &acct, 8);
  fake_setup.linksEntities[0].id = 21;
  fake_inventory.slots[0].id.value = 55;
  fake_inventory.slots[0].data.count = 3;
  emf::World w(reinterpret_cast<EmfWorld *>(1));
  auto r = request("inventory.remove");
  r["expectedInventory"] = J::array();
  bool rejected = false;
  try {
    execute(w, "stale", r);
  } catch (...) {
    rejected = true;
  }
  assert(rejected && mutations == 0);
  r = request("inventory.remove");
  r["account"] = "other";
  rejected = false;
  try {
    execute(w, "identity", r);
  } catch (...) {
    rejected = true;
  }
  assert(rejected && mutations == 0);
  r = request("inventory.remove");
  execute(w, "remove", r);
  assert(read_json(ipc / "results/remove.json")["ok"] == true);
  assert(fake_inventory.slots[0].data.count == 2);
  partial = true;
  r = request("inventory.add");
  r["count"] = 3;
  execute(w, "partial", r);
  auto response = read_json(ipc / "results/partial.json");
  assert(response["ok"] == false && response["data"]["partial"] == true &&
         response["data"]["completed"] == 2);
  r = request("teleport");
  r["position"] = {{"x", -1}, {"y", 0}, {"z", 0}};
  rejected = false;
  try {
    execute(w, "bounds", r);
  } catch (...) {
    rejected = true;
  }
  assert(rejected);
  r = {{"op", "settings.set"},
       {"session", session},
       {"key", "loot"},
       {"expected", 1.0},
       {"value", 1.5}};
  execute(w, "setting", r);
  assert(!fs::exists(ipc / "results/setting.json"));
  last_publish = last_requests = GetTickCount64();
  tick(w);
  assert(!fs::exists(ipc / "results/setting.json"));
  fake_setting = 1.5;
  tick(w);
  assert(read_json(ipc / "results/setting.json")["ok"] == true);
  write_json(fs::current_path() / "build/server-admin/native-test-result.json",
             {{"ok", true}, {"checks", 6}});
  fs::remove_all(ipc);
  std::cout << "PASS: stale inventory, identity, exact-slot remove, partial "
               "add, teleport bounds, deferred settings acknowledgement\n";
  return 0;
}
