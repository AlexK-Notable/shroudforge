#pragma once
struct KfcRuntimeHost;
namespace EngineHooks {
bool SetHost(const KfcRuntimeHost* host);  // Kiln fork: abi/runtime_host.h
bool Install();
void Uninstall();
}
