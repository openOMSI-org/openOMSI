// SPDX-License-Identifier: AGPL-3.0-only
// Public C ABI from the user's NGX SDK; the DLSSNR.* keys are NOT an official
// NVIDIA NR contract. Observed profile: UniversalDLSS5 369872ffb80a2bd8a4f8e3469f5b40e515fcc0f6,
// src/neural/ngx_nr.cpp (Apache-2.0). Independently written narrow adapter.
// Excludes upstream snippet fallback, borrowed application IDs, NVAPI hooks,
// GPU architecture substitutions and Vulkan/D3D11 interop.
#include "ngx.hpp"
#include "runtime_files.hpp"
#include <nvsdk_ngx.h>
#include <nvsdk_ngx_params.h>
#include <iostream>
#include <wrl/client.h>
#include <tlhelp32.h>
namespace omsinr {
namespace {
constexpr auto feature=NVSDK_NGX_Feature_Reserved18;
constexpr const char* engine="openOMSI-neural-host-1";
void check(NVSDK_NGX_Result r,const char* operation,Evidence& proof) {
    proof.phase=operation;proof.call("NGX",operation,uint32_t(r));
    if(NVSDK_NGX_FAILED(r))throw std::runtime_error(std::string(operation)+" NGX result="+std::to_string(uint32_t(r))+" "+code_hex(uint32_t(r)));
}
std::string support_reason(NVSDK_NGX_Feature_Support_Result result) {
    const auto mask=uint32_t(result);if(!mask)return "supported";
    const std::pair<NVSDK_NGX_Feature_Support_Result,const char*> fields[]={
        {NVSDK_NGX_FeatureSupportResult_CheckNotPresent,"check-not-present (support unproven)"},
        {NVSDK_NGX_FeatureSupportResult_DriverVersionUnsupported,"driver-version-unsupported"},
        {NVSDK_NGX_FeatureSupportResult_AdapterUnsupported,"adapter-unsupported"},
        {NVSDK_NGX_FeatureSupportResult_OSVersionBelowMinimumSupported,"OS-below-minimum"},
        {NVSDK_NGX_FeatureSupportResult_NotImplemented,"feature-query-not-implemented"}};
    std::string reason;uint32_t known=0;
    for(const auto& field:fields) {known|=uint32_t(field.first);if(mask&uint32_t(field.first)) {if(!reason.empty())reason+=", ";reason+=field.second;}}
    if(mask&~known)reason+=" unknown-bits="+code_hex(mask&~known);return reason;
}
NVSDK_NGX_Result NVSDK_CONV scale_callback(NVSDK_NGX_Parameter* params) {
    if(!params)return NVSDK_NGX_Result_FAIL_InvalidParameter;
    NVSDK_NGX_Parameter_SetF(params,"DLSSNR.ScalingRatio",1.f);
    return NVSDK_NGX_Result_Success;
}

}
struct Ngx18::Impl {
    Microsoft::WRL::ComPtr<ID3D12Device> device;NVSDK_NGX_Parameter* params{};NVSDK_NGX_Handle* handle{};
    HMODULE runtime_module{};bool started=false;
    std::filesystem::path runtime_path;
    ~Impl() {
        if(handle)NVSDK_NGX_D3D12_ReleaseFeature(handle);
        if(params)NVSDK_NGX_D3D12_DestroyParameters(params);
        if(started)NVSDK_NGX_D3D12_Shutdown1(device.Get());
        if(runtime_module)FreeLibrary(runtime_module);
    }
};
Ngx18::Ngx18(ID3D12Device* device,IDXGIAdapter* adapter,const std::filesystem::path& runtime,
             const std::filesystem::path& data,const std::string& project,Evidence& proof)
    :impl(std::make_unique<Impl>()) {
    impl->device=device;
    const auto dll=runtime/L"nvngx_dlssnr.dll";
    impl->runtime_path=dll;
    check_ngx_profile(dll);
    // RuntimeFiles verified/locked this exact file before reaching this constructor.
    impl->runtime_module=LoadLibraryExW(dll.c_str(),nullptr,LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);
    win32_check(impl->runtime_module!=nullptr,"LoadLibraryExW(nvngx_dlssnr.dll)");proof.mark(Loaded);
    wchar_t actual[32768]{};const auto length=GetModuleFileNameW(impl->runtime_module,actual,32768);
    require(length>0 && length<32768 && std::filesystem::equivalent(dll,std::filesystem::path(actual)),"NGX module origin mismatch");
    const auto path=runtime.wstring(),logs=data.wstring();const wchar_t* paths[]={path.c_str()};
    NVSDK_NGX_FeatureCommonInfo common{};common.PathListInfo.Path=paths;common.PathListInfo.Length=1;
    NVSDK_NGX_FeatureDiscoveryInfo discovery{};discovery.SDKVersion=NVSDK_NGX_Version_API;
    discovery.FeatureID=feature;discovery.ApplicationDataPath=logs.c_str();discovery.FeatureInfo=&common;
    discovery.Identifier.IdentifierType=NVSDK_NGX_Application_Identifier_Type_Project_Id;
    discovery.Identifier.v.ProjectDesc={project.c_str(),NVSDK_NGX_ENGINE_TYPE_CUSTOM,engine};
    NVSDK_NGX_FeatureRequirement requirements{};
    check(NVSDK_NGX_D3D12_GetFeatureRequirements(adapter,&discovery,&requirements),"NVSDK_NGX_D3D12_GetFeatureRequirements(Reserved18)",proof);
    proof.support_mask=uint32_t(requirements.FeatureSupported);proof.save_progress();
    const auto reason=support_reason(requirements.FeatureSupported);
    std::cout<<"NGX Feature18 support_mask="<<uint32_t(requirements.FeatureSupported)<<" reason="<<reason
             <<" minimum_architecture="<<requirements.MinHWArchitecture<<'\n';
    if(requirements.FeatureSupported!=NVSDK_NGX_FeatureSupportResult_Supported)
        throw std::runtime_error("NGX Feature18 support refused/inconclusive: "+reason+" mask="+code_hex(uint32_t(requirements.FeatureSupported)));
    proof.mark(Supported);
    check(NVSDK_NGX_D3D12_Init_with_ProjectID(project.c_str(),NVSDK_NGX_ENGINE_TYPE_CUSTOM,engine,
        logs.c_str(),device,&common,NVSDK_NGX_Version_API),"NVSDK_NGX_D3D12_Init_with_ProjectID",proof);
    impl->started=true;proof.core_initialized=true;verify_nr_origin(dll);
    check(NVSDK_NGX_D3D12_AllocateParameters(&impl->params),"NVSDK_NGX_D3D12_AllocateParameters",proof);
    require(impl->params!=nullptr,"NGX returned null parameters");
}
Ngx18::~Ngx18()=default;
void Ngx18::release() {
    if(impl->handle) {
        const auto result=NVSDK_NGX_D3D12_ReleaseFeature(impl->handle);
        require(NVSDK_NGX_SUCCEED(result),"NGX feature release failed");impl->handle=nullptr;
    }
}
void Ngx18::record(ID3D12GraphicsCommandList* list,const std::array<ID3D12Resource*,4>& resources,
                  uint32_t width,uint32_t height,bool reset,Evidence& proof,int32_t& result) {
    auto* p=impl->params;
    auto ui=[&](const char* key,unsigned value){NVSDK_NGX_Parameter_SetUI(p,key,value);};
    auto f=[&](const char* key,float value){NVSDK_NGX_Parameter_SetF(p,key,value);};
    if(!impl->handle) {
        ui(NVSDK_NGX_Parameter_Width,width);ui(NVSDK_NGX_Parameter_Height,height);
        ui(NVSDK_NGX_Parameter_CreationNodeMask,1);ui(NVSDK_NGX_Parameter_VisibilityNodeMask,1);
        NVSDK_NGX_Parameter_SetI(p,NVSDK_NGX_Parameter_PerfQualityValue,NVSDK_NGX_PerfQuality_Value_Balanced);
        for(const char* key:{"DLSSNR.Width","DLSSNR.InputWidth","DLSSNR.OutputWidth","DLSSNR.Output.Width"})ui(key,width);
        for(const char* key:{"DLSSNR.Height","DLSSNR.InputHeight","DLSSNR.OutputHeight","DLSSNR.Output.Height"})ui(key,height);
        ui("DLSSNR.Upscaling",0);f("DLSSNR.Scale",1);f("DLSSNR.ScalingRatio",1);
        NVSDK_NGX_Parameter_SetULL(p,"DLSSNRComputeScalingRatioCallback",reinterpret_cast<unsigned long long>(&scale_callback));
        ui("DLSSNR.Hint.Render.Preset",0); // DLL default; not a documented quality selector.
        const auto creation=NVSDK_NGX_D3D12_CreateFeature(list,feature,p,&impl->handle);
        proof.feature_creation_code=uint32_t(creation);proof.feature_creation_frame=proof.frame;
        check(creation,"NVSDK_NGX_D3D12_CreateFeature(Reserved18)",proof);
        require(impl->handle!=nullptr,"NGX returned null Feature18 handle");
        proof.feature_created=true;
    }
    proof.mark(Initialized);
    const char* keys[]={"DLSSNR.Color","DLSSNR.MVec","DLSSNR.Depth","DLSSNR.Output"};
    for(size_t i=0;i<4;++i)NVSDK_NGX_Parameter_SetD3d12Resource(p,keys[i],resources[i]);
    NVSDK_NGX_Parameter_SetD3d12Resource(p,"DLSSNR.ControlMask",nullptr);
    f("DLSSNR.MVecScaleX",1);f("DLSSNR.MVecScaleY",1); // engine vectors already in pixels.
    ui("DLSSNR.DepthInverted",1);ui("DLSSNR.Enabled",1);ui("DLSSNR.Reset",reset?1:0);
    ui("DLSS.Indicator.Invert.X.Axis",0);ui("DLSS.Indicator.Invert.Y.Axis",0);
    ui("DLSSNR.Style",1);f("DLSSNR.Intensity",0.85f);f("DLSSNR.LocalToneStrength",0.45f);
    f("DLSSNR.LocalStructureStrength",1);f("DLSSNR.SkinStructureStrength",-1);
    ui("DLSSNR.UseAutoMask",1);ui("DLSSNR.UICorrection",1);
    for(const char* plane:{"Color","Output","MVec","Depth"}) {
        const auto prefix=std::string("DLSSNR.")+plane+"Subrect";
        ui((prefix+"BaseX").c_str(),0);ui((prefix+"BaseY").c_str(),0);
        ui((prefix+"Width").c_str(),width);ui((prefix+"Height").c_str(),height);
    }
    const auto r=NVSDK_NGX_D3D12_EvaluateFeature(list,impl->handle,p,nullptr);
    result=static_cast<int32_t>(r);check(r,"NVSDK_NGX_D3D12_EvaluateFeature(Reserved18)",proof);
    verify_nr_origin(impl->runtime_path);
}
}
