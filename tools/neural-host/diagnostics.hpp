#pragma once
#include "streamline.hpp"
#include "runtime_files.hpp"
#include <dxgi1_6.h>
#include <d3d12.h>
#include <wrl/client.h>
namespace omsinr {
inline void diagnose(const std::filesystem::path& runtime) {
    using Microsoft::WRL::ComPtr;
    using RtlGetVersionFn=LONG(WINAPI*)(OSVERSIONINFOW*);
    const auto rtl=reinterpret_cast<RtlGetVersionFn>(GetProcAddress(GetModuleHandleW(L"ntdll.dll"),"RtlGetVersion"));
    OSVERSIONINFOW os{};os.dwOSVersionInfoSize=sizeof os;
    if(rtl && rtl(&os)==0)std::cout<<"Windows="<<os.dwMajorVersion<<'.'<<os.dwMinorVersion<<" build="<<os.dwBuildNumber<<'\n';
    std::cout<<"Execution environment="<<(GetProcAddress(GetModuleHandleW(L"ntdll.dll"),"wine_get_version")?"Wine (not native Windows validation)":"Windows")<<'\n';
    ComPtr<IDXGIFactory4> factory;
    const auto factory_result=CreateDXGIFactory1(IID_PPV_ARGS(&factory));
    if(SUCCEEDED(factory_result))for(UINT i=0;;++i) {
        ComPtr<IDXGIAdapter1> adapter;const auto enumeration=factory->EnumAdapters1(i,&adapter);
        if(enumeration==DXGI_ERROR_NOT_FOUND)break;
        if(FAILED(enumeration)) {std::cout<<"DXGI enumeration failed="<<enumeration<<'\n';break;}
        DXGI_ADAPTER_DESC1 d{};if(FAILED(adapter->GetDesc1(&d)))continue;
        LARGE_INTEGER driver{};const auto driver_result=adapter->CheckInterfaceSupport(__uuidof(IDXGIDevice),&driver);
        std::wcout<<L"Adapter="<<d.Description<<L" vendor="<<d.VendorId<<L" device="<<d.DeviceId
            <<L" VRAM_bytes="<<d.DedicatedVideoMemory<<L" LUID="<<d.AdapterLuid.HighPart<<L":"<<d.AdapterLuid.LowPart<<L'\n';
        if(SUCCEEDED(driver_result))std::cout<<"Driver version="<<HIWORD(driver.HighPart)<<'.'<<LOWORD(driver.HighPart)<<'.'<<HIWORD(driver.LowPart)<<'.'<<LOWORD(driver.LowPart)<<'\n';
        else std::cout<<"Driver version=query unavailable HRESULT="<<driver_result<<'\n';
        const auto directx=D3D12CreateDevice(adapter.Get(),D3D_FEATURE_LEVEL_12_0,__uuidof(ID3D12Device),nullptr);
        std::cout<<"D3D12 feature level 12.0="<<(SUCCEEDED(directx)?"available":"unavailable")
                 <<"; NR support=not queried (requires explicit runtime initialization)\n";
    } else std::cout<<"DXGI unavailable HRESULT="<<factory_result<<'\n';
    std::cout<<"Runtime directory="<<runtime.u8string()<<'\n';
    for(const char* name:{"sl.interposer.dll","sl.common.dll","sl.dlss_nr.dll","nvngx_dlssnr.dll"}) {
        const auto file=runtime/name;
        std::cout<<name<<" present="<<std::filesystem::is_regular_file(file);
        if(std::filesystem::is_regular_file(file)) {
            std::cout<<" version="<<file_version(file)<<'\n';
            try {RuntimeFiles verification(runtime,name);}
            catch(const std::exception& e){std::cout<<"Validation refused (PE/signature/dependencies): "<<e.what()<<'\n';}
        } else std::cout<<" version=unavailable signature=not checked sha256=unavailable\n";
    }
    std::cout<<"Compiled backends=Streamline NR"<<(ngx_compiled()?", NGX Feature18 experimental":"")<<'\n'
             <<"Initialization=not attempted; last error=none; Active=false\n";
}
}
