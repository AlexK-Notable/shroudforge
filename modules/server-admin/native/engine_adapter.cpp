// Local administration only. Engine mutations use EMF transactions on its
// server-world callback.
#define NOMINMAX
#include "emf/emf.hpp"
#include "emf/inventory.hpp"
#include "emf/teleport.hpp"
#include "json.hpp"
#include <chrono>
#include <cmath>
#include <filesystem>
#include <fstream>
#include <map>
#include <mutex>
#include <set>
#include <windows.h>
using J = nlohmann::json;
namespace fs = std::filesystem;
namespace gen = emf::gen::ecs;
namespace {
fs::path root, ipc;
std::string session;
uint32_t world_slot = 0;
std::mutex save_mutex;
std::set<std::string> save_waiters;
struct SettingPending {
  std::string key;
  double wanted;
  uint64_t deadline;
  bool persist;
};
std::map<std::string, SettingPending> pending_settings;
std::map<std::string, J> last_positions;
J graves = J::object();
uint64_t last_publish = 0, last_requests = 0;
uint32_t previous_tick = 0;
bool valid_id(const std::string &s) {
  return s.size() == 32 &&
         s.find_first_not_of("0123456789abcdef") == std::string::npos;
}
double epoch() {
  return std::chrono::duration<double>(
             std::chrono::system_clock::now().time_since_epoch())
      .count();
}
void write_json(const fs::path &p, const J &j) {
  fs::create_directories(p.parent_path());
  auto tmp = p;
  tmp += L".tmp";
  {
    std::ofstream f(tmp, std::ios::binary | std::ios::trunc);
    if (!f)
      throw std::runtime_error("cannot write response");
    f << j.dump();
    f.flush();
    if (!f)
      throw std::runtime_error("response write failed");
  }
  if (!MoveFileExW(tmp.c_str(), p.c_str(),
                   MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH))
    throw std::runtime_error("response publish failed");
}
J read_json(const fs::path &p) {
  std::ifstream f(p, std::ios::binary);
  return J::parse(f);
}
void result(const std::string &id, bool ok, const J &data) {
  write_json(ipc / "results" / (id + ".json"),
             {{"id", id}, {"ok", ok}, {"data", data}, {"finishedAt", epoch()}});
}
J position_json(emf::World w, uint32_t e) {
  auto p = emf::position(w, e);
  return p ? J{{"x", p->x}, {"y", p->y}, {"z", p->z}} : J();
}
std::string account(emf::World w, uint32_t e) {
  return std::to_string(emf::player_account(w, e));
}
J stacks(emf::World w, uint32_t e) {
  J out = J::array();
  for (auto s : emf::inventory_stacks(w, e)) {
    EmfItemInfo info{};
    info.size = sizeof info;
    emf::api()->item_info(w.raw(), s.item, &info);
    out.push_back({{"inventory", s.inventory},
                   {"slot", s.slot},
                   {"item", s.item},
                   {"count", s.count},
                   {"dataEntity", s.data_entity},
                   {"name", info.name},
                   {"maxStack", info.max_stack}});
  }
  return out;
}
void load_graves() {
  try {
    graves =
        read_json(ipc / ("graves-" + std::to_string(world_slot) + ".json"));
    if (!graves.is_object())
      graves = J::object();
  } catch (...) {
    graves = J::object();
  }
}
void publish(emf::World w) {
  J players = J::array();
  for (auto e : emf::players(w)) {
    auto a = account(w, e);
    auto p = position_json(w, e);
    if (a == "0" || p.is_null())
      continue;
    last_positions[a] = p;
    players.push_back({{"entity", e},
                       {"account", a},
                       {"name", emf::player_name(w, e).value_or(
                                    "Player " + std::to_string(e))},
                       {"position", p},
                       {"inventory", stacks(w, e)}});
  }
  J settings = J::array();
  EmfWorldSettingInfo i{};
  for (uint32_t n = 0; n < 128; n++) {
    i = {};
    i.size = sizeof i;
    if (!emf::api()->world_setting_info(n, &i))
      break;
    settings.push_back({{"key", i.key},
                        {"group", i.group},
                        {"value", i.value},
                        {"baseline", i.world},
                        {"min", i.min},
                        {"max", i.max},
                        {"flags", i.flags},
                        {"type", i.type}});
  }
  write_json(ipc / "state.json", {{"schemaVersion", 1},
                                  {"session", session},
                                  {"worldSlot", world_slot},
                                  {"updatedAt", epoch()},
                                  {"tick", w.tick()},
                                  {"players", players},
                                  {"graves", graves},
                                  {"settings", settings},
                                  {"capabilities",
                                   {{"inventory", true},
                                    {"teleport", emf::has_v20()},
                                    {"worldSettings", emf::has_v21()},
                                    {"save", emf::has_v19()}}}});
}
uint32_t player(emf::World w, const J &r) {
  if (r.at("session").get<std::string>() != session)
    throw std::runtime_error(
        "Server session changed. Refresh before retrying.");
  auto e = r.at("entity").get<uint32_t>();
  auto a = r.at("account").get<std::string>();
  auto ps = emf::players(w);
  if (a == "0" || std::find(ps.begin(), ps.end(), e) == ps.end() ||
      account(w, e) != a)
    throw std::runtime_error("Player disconnected or entity identity changed.");
  return e;
}
void teleport(emf::World w, const std::string &id, uint32_t e, J target) {
  double x = target.at("x"), y = target.at("y"), z = target.at("z");
  if (!std::isfinite(x) || !std::isfinite(y) || !std::isfinite(z) || x < 0 ||
      x > 10240 || z < 0 || z > 10240 || y < -1000 || y > 5000)
    throw std::runtime_error("Destination outside supported world bounds.");
  emf::TeleportOptions o;
  o.ground = true;
  o.checkpoint = true;
  o.notify_ready = true;
  bool queued = emf::teleport(
      w, e, {x, y, z}, o, [id](emf::World, const emf::TeleportResult &r) {
        if (!r.last)
          return;
        result(id, r.moved() && !r.session_ended,
               {{"status", r.status},
                {"position", {{"x", r.pos.x}, {"y", r.pos.y}, {"z", r.pos.z}}},
                {"reason", r.reason},
                {"offTarget", r.off_target},
                {"rawTarget", r.raw_target}});
      });
  if (!queued)
    throw std::runtime_error(
        "Engine refused teleport request; nothing queued.");
}
void execute(emf::World w, const std::string &id, const J &r) {
  auto op = r.at("op").get<std::string>();
  if (op == "ping") {
    result(id, true, {{"api", emf::api()->version}});
    return;
  }
  if (r.at("session").get<std::string>() != session)
    throw std::runtime_error(
        "Server session changed. Refresh before retrying.");
  if (op == "inventory.add" || op == "inventory.remove") {
    auto e = player(w, r);
    auto before = stacks(w, e);
    if (before != r.at("expectedInventory"))
      throw std::runtime_error(
          "Inventory changed since displayed. Refresh and retry.");
    auto amount = r.at("count").get<int64_t>();
    if (amount < 1 || amount > 65535)
      throw std::runtime_error("Count must be 1..65535.");
    uint32_t item = 0;
    if (r.contains("item"))
      item = r.at("item").get<uint32_t>();
    else
      item = emf::api()->find_item(w.raw(),
                                   r.at("itemName").get<std::string>().c_str());
    EmfItemInfo info{};
    info.size = sizeof info;
    if (!item || !emf::api()->item_info(w.raw(), item, &info))
      throw std::runtime_error("Unknown item. Use the item catalog.");
    auto callback = [id, op, e, item, amount](emf::World world, uint32_t done) {
      result(id, done == amount,
             {{"operation", op},
              {"requested", amount},
              {"completed", done},
              {"partial", done > 0 && done != amount},
              {"inventory", world ? stacks(world, e) : J::array()}});
    };
    bool ok = false;
    if (op == "inventory.add")
      ok = emf::put_items_ex(w, e, item, (uint32_t)amount, 0, 0xff, callback);
    else {
      auto inv = r.at("inventory").get<uint32_t>();
      auto slot = r.at("slot").get<int>();
      bool own = false;
      for (auto &s : before)
        if (s["inventory"] == inv && s["slot"] == slot && s["item"] == item &&
            s["count"].get<int64_t>() >= amount)
          own = true;
      if (!own || slot < 0 || slot > 7)
        throw std::runtime_error(
            "Slot no longer contains enough of the selected item.");
      ok = emf::take_items(w, e, item, (uint32_t)amount,
                           emf::slot_id(inv, (uint8_t)slot), callback, true);
    }
    if (!ok)
      throw std::runtime_error(
          "Engine refused inventory transaction; nothing queued.");
    return;
  }
  if (op == "teleport" || op == "teleport.grave") {
    auto e = player(w, r);
    J target;
    if (op == "teleport.grave") {
      auto a = r.at("account").get<std::string>();
      if (!graves.contains(a))
        throw std::runtime_error(
            "No death site recorded for this player in this world.");
      auto grave = graves.at(a);
      if (grave.at("recordedAt") != r.at("expectedGraveTime"))
        throw std::runtime_error(
            "A newer death site was recorded; refresh first.");
      target = grave.at("position");
      target["y"] = target["y"].get<double>() + 2;
    } else
      target = r.at("position");
    teleport(w, id, e, target);
    return;
  }
  if (op == "settings.set" || op == "settings.reset") {
    auto key = r.at("key").get<std::string>();
    auto current = emf::world_setting(key.c_str());
    if (!current || std::abs(*current - r.at("expected").get<double>()) > 1e-7)
      throw std::runtime_error(
          "Setting changed since displayed. Refresh first.");
    bool persist = r.value("persist", false), ok = false;
    if (op == "settings.reset")
      ok = emf::reset_world_setting(key.c_str(), persist);
    else {
      double value = r.at("value");
      if (!std::isfinite(value))
        throw std::runtime_error("Setting must be finite.");
      ok = emf::set_world_setting(key.c_str(), value, persist);
    }
    if (!ok)
      throw std::runtime_error(
          "Engine rejected setting or persistence request.");
    double wanted = r.value("value", 0.0);
    if (op == "settings.reset") {
      EmfWorldSettingInfo info{};
      for (uint32_t n = 0; n < 128; n++) {
        info = {};
        info.size = sizeof info;
        if (!emf::api()->world_setting_info(n, &info))
          break;
        if (key == info.key) {
          wanted = info.world;
          break;
        }
      }
    }
    pending_settings[id] = {key, wanted, GetTickCount64() + 5000, persist};
    return;
  }
  if (op == "save") {
    {
      std::lock_guard<std::mutex> lock(save_mutex);
      save_waiters.insert(id);
    }
    if (!emf::request_save()) {
      std::lock_guard<std::mutex> lock(save_mutex);
      save_waiters.erase(id);
      throw std::runtime_error("Engine refused save request.");
    }
    return;
  }
  if (op == "items") {
    J items = J::array();
    emf::api()->for_each_item(
        w.raw(),
        [](const EmfItemInfo *i, void *p) {
          static_cast<J *>(p)->push_back({{"item", i->item_id},
                                          {"name", i->name},
                                          {"maxStack", i->max_stack}});
        },
        &items);
    write_json(ipc / "items.json", items);
    result(id, true, {{"count", items.size()}});
    return;
  }
  throw std::runtime_error("Unknown administration operation.");
}
void tick(emf::World w) {
  if (!w.is_server())
    return;
  auto now = GetTickCount64();
  for (auto it = pending_settings.begin(); it != pending_settings.end();) {
    auto &change = it->second;
    auto value = emf::world_setting(change.key.c_str());
    bool applied = value && std::abs(*value - change.wanted) < 1e-5;
    if (applied || now > change.deadline) {
      result(
          it->first, applied,
          {{"key", change.key},
           {"value", value.value_or(0)},
           {"requested", change.wanted},
           {"persistRequested", change.persist},
           {"error",
            applied ? ""
                    : "Setting was queued but readback did not confirm it."}});
      it = pending_settings.erase(it);
    } else
      ++it;
  }
  // A reused world pointer must not reuse a command session across
  // load/restart.
  if (w.tick() < previous_tick) {
    session = std::to_string(GetCurrentProcessId()) + "-" + std::to_string(now);
    last_positions.clear();
  }
  previous_tick = w.tick();
  if (now - last_publish >= 1000) {
    try {
      publish(w);
    } catch (const std::exception &e) {
      EMF_LOG("server-admin", "snapshot failed: %s", e.what());
    }
    last_publish = now;
  }
  if (now - last_requests < 100)
    return;
  last_requests = now;
  std::error_code ec;
  auto dir = ipc / "requests";
  fs::create_directories(dir, ec);
  int processed = 0;
  for (auto &entry : fs::directory_iterator(dir, ec)) {
    if (++processed > 4)
      break;
    auto p = entry.path();
    if (p.extension() != L".json")
      continue;
    auto id = p.stem().string();
    if (!valid_id(id))
      continue;
    if (fs::exists(ipc / "results" / (id + ".json"))) {
      fs::remove(p, ec);
      continue;
    }
    auto claimed = p;
    claimed += L".processing";
    fs::rename(p, claimed, ec);
    if (ec)
      continue;
    try {
      if (fs::file_size(claimed) > 128 * 1024)
        throw std::runtime_error("Request too large.");
      auto r = read_json(claimed);
      double age = epoch() - r.at("createdAt").get<double>();
      if (age < -5 || age > 30)
        throw std::runtime_error("Request expired; no action performed.");
      if (r.at("id") != id)
        throw std::runtime_error("Request ID mismatch.");
      execute(w, id, r);
    } catch (const std::exception &e) {
      try {
        result(id, false, {{"error", e.what()}});
      } catch (...) {
      }
    }
    fs::remove(claimed, ec);
  }
}
} // namespace
extern "C" __declspec(dllexport) bool emf_mod_load(const EmfApi *a) {
  if (!emf::init(a) || !emf::has_v21())
    return false;
  wchar_t exe[32768];
  GetModuleFileNameW(nullptr, exe, 32768);
  root = fs::path(exe).parent_path();
  ipc = root / L"shroudforge/runtime/admin/engine";
  fs::create_directories(ipc / "results");
  session = std::to_string(GetCurrentProcessId()) + "-" +
            std::to_string(GetTickCount64());
  a->on_load(
      [](uint16_t slot, void *) {
        world_slot = slot;
        last_positions.clear();
        previous_tick = 0;
        session = std::to_string(GetCurrentProcessId()) + "-" +
                  std::to_string(GetTickCount64());
        load_graves();
      },
      nullptr);
  a->on_save(
      [](uint16_t slot, void *) {
        std::lock_guard<std::mutex> lock(save_mutex);
        for (auto &id : save_waiters)
          try {
            result(id, true, {{"saved", true}, {"worldSlot", slot}});
          } catch (...) {
          }
        save_waiters.clear();
      },
      nullptr);
  emf::on_event<gen::EntityDiedEvent>(
      EMF_WORLD_SERVER, [](emf::World w, const gen::EntityDiedEvent &event) {
        auto e = event.deadEntityId.id;
        if (!w.get<gen::PlayerState>(e))
          return;
        auto a = account(w, e);
        if (a == "0")
          return;
        auto p = position_json(w, e);
        if (p.is_null() && last_positions.count(a))
          p = last_positions[a];
        if (p.is_null())
          return;
        graves[a] = {{"position", p},
                     {"recordedAt", epoch()},
                     {"source", "EntityDiedEvent"},
                     {"worldSlot", world_slot}};
        try {
          write_json(ipc / ("graves-" + std::to_string(world_slot) + ".json"),
                     graves);
        } catch (...) {
        }
      });
  emf::on_tick([](emf::World w) {
    try {
      tick(w);
    } catch (const std::exception &e) {
      EMF_LOG("server-admin", "administration step failed: %s", e.what());
    }
  });
  EMF_LOG("server-admin",
          "administration adapter active; no automatic gameplay changes");
  return true;
}
