#pragma once
#include "protocol.hpp"
#include "evidence.hpp"
#include <d3d12.h>
#include <dxgi.h>
#include <filesystem>
#include <memory>
namespace omsinr {
// Optional public NGX dispatcher. No private exported-function casts or hooks.
class Ngx18 {
    struct Impl;
    std::unique_ptr<Impl> impl;
public:
    Ngx18(ID3D12Device*,IDXGIAdapter*,const std::filesystem::path& runtime,
          const std::filesystem::path& data,const std::string& project,Evidence&);
    ~Ngx18();
    void release();
    void record(ID3D12GraphicsCommandList*,const std::array<ID3D12Resource*,4>&,
                uint32_t width,uint32_t height,bool reset,Evidence&,int32_t&);
};
}
