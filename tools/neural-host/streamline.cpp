// SPDX-License-Identifier: AGPL-3.0-only
// Shared D3D12 staging and the public Streamline 2.14.16 path. The optional NGX
// adapter is isolated in ngx.cpp. NR contracts and native Windows remain unqualified.
#include "streamline.hpp"
#include "runtime_files.hpp"
#ifdef OPENOMSI_WITH_NGX
#include "ngx.hpp"
#endif
#include <windows.h>
#include <d3d12.h>
#include <dxgi1_6.h>
#include <wintrust.h>
#include <softpub.h>
#include <wincrypt.h>
#include <bcrypt.h>
#include <wrl/client.h>
#include <sl.h>
#include <iostream>
#include <iomanip>
#include <sstream>
#include <cwctype>

using Microsoft::WRL::ComPtr;
namespace omsinr {
namespace {
D3D12_HEAP_PROPERTIES heap(D3D12_HEAP_TYPE type) {D3D12_HEAP_PROPERTIES h{};h.Type=type;h.CreationNodeMask=h.VisibleNodeMask=1;return h;}
D3D12_RESOURCE_BARRIER transition(ID3D12Resource* resource,D3D12_RESOURCE_STATES from,D3D12_RESOURCE_STATES to) {
    D3D12_RESOURCE_BARRIER b{};b.Type=D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    b.Transition.pResource=resource;b.Transition.StateBefore=from;b.Transition.StateAfter=to;
    b.Transition.Subresource=D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;return b;
}
struct Plane {
    ComPtr<ID3D12Resource> texture,upload,readback;
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT footprint{};
    uint32_t bpp{};uint64_t bytes{};
};
}

struct NeuralHost::Impl {
    Evidence& proof;
    Backend backend;
    RuntimeFiles files;
    std::filesystem::path runtime_root;
#ifdef OPENOMSI_WITH_NGX
    std::unique_ptr<Ngx18> ngx;
#endif
    HMODULE module{};bool sl_initialized=false,feature_initialized=false;
    DLL_DIRECTORY_COOKIE dll_directory{};
    ComPtr<IDXGIFactory4> factory,proxy_factory;
    ComPtr<ID3D12Device> device,proxy_device;
    ComPtr<ID3D12CommandQueue> queue;
    ComPtr<ID3D12CommandAllocator> allocator;
    ComPtr<ID3D12GraphicsCommandList> list;
    ComPtr<ID3D12Fence> fence;
    ComPtr<IDXGISwapChain1> swapchain;
    Handle event;HWND window{};uint64_t fence_value{};
    std::array<Plane,4> planes;uint32_t width{},height{},generation{};
    uint64_t previous_session{},previous_frame{};
    const sl::ViewportHandle viewport{1};
    PFun_slInit* init{};PFun_slShutdown* shutdown{};PFun_slSetD3DDevice* set_device{};
    PFun_slIsFeatureSupported* supported{};PFun_slGetFeatureRequirements* requirements{};
    PFun_slIsFeatureLoaded* feature_loaded{};PFun_slGetFeatureVersion* feature_version{};
    PFun_slGetNewFrameToken* token{};PFun_slSetConstants* constants{};
    PFun_slSetTagForFrame* tags{};PFun_slEvaluateFeature* evaluate{};
    PFun_slAllocateResources* allocate{};PFun_slFreeResources* free_resources{};PFun_slUpgradeInterface* upgrade{};
    int32_t last_result=-1;

    template<class T>void symbol(T*& out,const char* name){const auto operation=std::string("GetProcAddress(")+name+")";out=reinterpret_cast<T*>(GetProcAddress(module,name));win32_check(out!=nullptr,operation.c_str());}
    void slcheck(sl::Result r,const char* label) {proof.phase=label;proof.call("Streamline",label,int32_t(r));last_result=int32_t(r);if(r!=sl::Result::eOk){std::cerr<<label<<" result="<<last_result<<'\n';throw std::runtime_error(label);}}
    Impl(const std::filesystem::path& runtime,const std::string& project,Backend selected,Evidence& evidence,const std::filesystem::path& data)
        :proof(evidence),backend(selected),files(runtime,selected==Backend::Ngx) {
        runtime_root=runtime;
        proof.runtime_directory=runtime.u8string();proof.runtime_sha256=files.digests.at(L"nvngx_dlssnr.dll");
        proof.runtime_version=file_version(runtime/L"nvngx_dlssnr.dll");
        proof.mark(Verified);
        require(project.size()==36 && project.find_first_not_of("0123456789abcdefABCDEF-")==std::string::npos,"provide your own NVIDIA project GUID");
        win32_check(SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32|LOAD_LIBRARY_SEARCH_USER_DIRS)!=0,"SetDefaultDllDirectories");
        dll_directory=AddDllDirectory(runtime.c_str());win32_check(dll_directory!=nullptr,"AddDllDirectory(runtime)");
        if(backend==Backend::Streamline) {
        module=LoadLibraryExW((runtime/L"sl.interposer.dll").c_str(),nullptr,LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);
        win32_check(module!=nullptr,"LoadLibraryExW(sl.interposer.dll)");
        symbol(init,"slInit");symbol(shutdown,"slShutdown");symbol(set_device,"slSetD3DDevice");
        symbol(supported,"slIsFeatureSupported");symbol(requirements,"slGetFeatureRequirements");
        symbol(feature_loaded,"slIsFeatureLoaded");symbol(feature_version,"slGetFeatureVersion");
        symbol(token,"slGetNewFrameToken");symbol(constants,"slSetConstants");symbol(tags,"slSetTagForFrame");
        symbol(evaluate,"slEvaluateFeature");symbol(allocate,"slAllocateResources");symbol(free_resources,"slFreeResources");symbol(upgrade,"slUpgradeInterface");
        const auto path=runtime.wstring();const wchar_t* paths[]={path.c_str()};const sl::Feature features[]={sl::kFeatureDLSS_NR};
        sl::Preferences preferences{};preferences.pathsToPlugins=paths;preferences.numPathsToPlugins=1;
        preferences.featuresToLoad=features;preferences.numFeaturesToLoad=1;
        preferences.engine=sl::EngineType::eCustom;preferences.engineVersion="openOMSI-external-prototype-1";
        preferences.projectId=project.c_str();preferences.renderAPI=sl::RenderAPI::eD3D12;
        // No OTA, OS bypass, hardware spoof, DLSS SR/FG substitution or copied app ID.
        preferences.flags=sl::PreferenceFlags::eUseManualHooking|sl::PreferenceFlags::eUseFrameBasedResourceTagging|sl::PreferenceFlags::eDisableCLStateTracking;
        preferences.logMessageCallback=[](sl::LogType,const char* msg){std::cerr<<"Streamline: "<<msg<<'\n';};
        slcheck(init(preferences,sl::kSDKVersion),"slInit NR");sl_initialized=true;proof.core_initialized=true;
        }
        hr(CreateDXGIFactory1(IID_PPV_ARGS(&factory)),"CreateDXGIFactory1");
        ComPtr<IDXGIAdapter1> adapter;DXGI_ADAPTER_DESC1 desc{};
        bool found=false;for(UINT index=0;factory->EnumAdapters1(index,&adapter)!=DXGI_ERROR_NOT_FOUND;++index) {
            hr(adapter->GetDesc1(&desc),"adapter description failed");
            if(desc.VendorId==0x10de && !(desc.Flags&DXGI_ADAPTER_FLAG_SOFTWARE)){found=true;break;}adapter.Reset();
        }
        require(found,"NVIDIA adapter missing");
        char gpu_name[1024]{};WideCharToMultiByte(CP_UTF8,0,desc.Description,-1,gpu_name,1024,nullptr,nullptr);
        proof.gpu_name=gpu_name;proof.gpu_vendor=desc.VendorId;proof.gpu_device=desc.DeviceId;
        proof.vram_bytes=desc.DedicatedVideoMemory;
        proof.luid=std::to_string(desc.AdapterLuid.HighPart)+":"+std::to_string(desc.AdapterLuid.LowPart);
        LARGE_INTEGER driver{};const auto driver_result=adapter->CheckInterfaceSupport(__uuidof(IDXGIDevice),&driver);
        proof.call("HRESULT","IDXGIAdapter::CheckInterfaceSupport(driver)",int32_t(driver_result));
        if(SUCCEEDED(driver_result))proof.driver_version=std::to_string(HIWORD(driver.HighPart))+"."+std::to_string(LOWORD(driver.HighPart))+"."+std::to_string(HIWORD(driver.LowPart))+"."+std::to_string(LOWORD(driver.LowPart));
        proof.save_progress();
        hr(D3D12CreateDevice(adapter.Get(),D3D_FEATURE_LEVEL_12_0,IID_PPV_ARGS(&device)),"D3D12CreateDevice(FL12_0)");
        if(backend==Backend::Streamline) {
        slcheck(set_device(device.Get()),"slSetD3DDevice");
        bool mounted=false;slcheck(feature_loaded(sl::kFeatureDLSS_NR,mounted),"NR plugin loaded query");require(mounted,"NR plugin was not loaded");
        if(GetModuleHandleW(L"nvngx_dlssnr.dll")) {verify_nr_origin(runtime/L"nvngx_dlssnr.dll");proof.mark(Loaded);}
        sl::FeatureVersion version{};slcheck(feature_version(sl::kFeatureDLSS_NR,version),"NR version query");
        std::cout<<"NR plugin SL="<<version.versionSL.major<<'.'<<version.versionSL.minor<<'.'<<version.versionSL.build
            <<" NGX="<<version.versionNGX.major<<'.'<<version.versionNGX.minor<<'.'<<version.versionNGX.build<<'\n';
        sl::AdapterInfo info{};info.deviceLUID=reinterpret_cast<uint8_t*>(&desc.AdapterLuid);info.deviceLUIDSizeInBytes=sizeof desc.AdapterLuid;
        slcheck(supported(sl::kFeatureDLSS_NR,info),"NR GPU/driver support query");
        sl::FeatureRequirements req{};slcheck(requirements(sl::kFeatureDLSS_NR,req),"NR requirements");
        require(bool(req.flags&sl::FeatureRequirementFlags::eD3D12Supported),"NR requires another API");
        require(req.numRequiredTags==0 || req.requiredTags!=nullptr,"invalid required tags");
        for(uint32_t i=0;i<req.numRequiredTags;++i) {
            const auto t=req.requiredTags[i];
            require(t==sl::kBufferTypeUpliftInputColor || t==sl::kBufferTypeUpliftOutputColor
                || t==sl::kBufferTypeDepth || t==sl::kBufferTypeMotionVectors,"unsupported required NR resource tag");
        }
        proof.mark(Supported);
        proxy_device=device;slcheck(upgrade(reinterpret_cast<void**>(proxy_device.GetAddressOf())),"device proxy");
        proxy_factory=factory;slcheck(upgrade(reinterpret_cast<void**>(proxy_factory.GetAddressOf())),"factory proxy");
        } else {
#ifdef OPENOMSI_WITH_NGX
            ngx=std::make_unique<Ngx18>(device.Get(),adapter.Get(),runtime,data,project,proof);
#else
            throw std::runtime_error("NGX backend not compiled; rebuild with OPENOMSI_ENABLE_NGX=ON and MSVC");
#endif
            proxy_device=device;proxy_factory=factory;
        }
        D3D12_COMMAND_QUEUE_DESC q{};q.Type=D3D12_COMMAND_LIST_TYPE_DIRECT;
        hr(proxy_device->CreateCommandQueue(&q,IID_PPV_ARGS(&queue)),"DIRECT queue failed");
        hr(device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT,IID_PPV_ARGS(&allocator)),"allocator failed");
        hr(device->CreateCommandList(0,D3D12_COMMAND_LIST_TYPE_DIRECT,allocator.Get(),nullptr,IID_PPV_ARGS(&list)),"command list failed");
        hr(list->Close(),"initial command list close failed");
        hr(device->CreateFence(0,D3D12_FENCE_FLAG_NONE,IID_PPV_ARGS(&fence)),"fence failed");
        event.h=CreateEventW(nullptr,FALSE,FALSE,nullptr);require(event.h!=nullptr,"fence event failed");
        // The proxy Present invokes common-plugin frame housekeeping even though all
        // inference textures are off-screen. No swapchain or wgpu hooks in the game.
        if(backend==Backend::Streamline) {
        window=CreateWindowExW(0,L"STATIC",L"openOMSI Neural Host",WS_OVERLAPPEDWINDOW,0,0,64,64,nullptr,nullptr,GetModuleHandleW(nullptr),nullptr);
        require(window!=nullptr,"host housekeeping window failed");
        DXGI_SWAP_CHAIN_DESC1 swap{};swap.Width=64;swap.Height=64;swap.Format=DXGI_FORMAT_R8G8B8A8_UNORM;
        swap.SampleDesc.Count=1;swap.BufferUsage=DXGI_USAGE_RENDER_TARGET_OUTPUT;swap.BufferCount=2;swap.SwapEffect=DXGI_SWAP_EFFECT_FLIP_DISCARD;
        hr(proxy_factory->CreateSwapChainForHwnd(queue.Get(),window,&swap,nullptr,nullptr,&swapchain),"proxy swapchain failed");
        }
        std::wcout<<L"NR support query accepted GPU: "<<desc.Description<<L"; no frame evaluated\n";
    }
    void drain() {
        hr(queue->Signal(fence.Get(),++fence_value),"GPU signal failed");
        hr(fence->SetEventOnCompletion(fence_value,event.h),"GPU completion event failed");
        if(WaitForSingleObject(event.h,10000)!=WAIT_OBJECT_0) {
            // Do not unload a DLL or release resources still used by the GPU.
            std::cerr<<"GPU timeout; terminating isolated host without in-process unload\n";
            ExitProcess(3);
        }
        hr(device->GetDeviceRemovedReason(),"ID3D12Device::GetDeviceRemovedReason");
    }
    void create_planes(uint32_t w,uint32_t h) {
        if(width==w && height==h)return;
        drain();
        if(feature_initialized && backend==Backend::Streamline) {slcheck(free_resources(sl::kFeatureDLSS_NR,viewport),"free NR resources");feature_initialized=false;}
#ifdef OPENOMSI_WITH_NGX
        if(ngx)ngx->release();
#endif
        proof.feature_created=false;proof.phase="D3D12CreateCommittedResource";proof.save_progress();
        width=w;height=h;
        const DXGI_FORMAT formats[]={DXGI_FORMAT_R8G8B8A8_UNORM,DXGI_FORMAT_R32G32_FLOAT,DXGI_FORMAT_R32_FLOAT,DXGI_FORMAT_R8G8B8A8_UNORM};
        for(size_t i=0;i<4;++i) {
            auto& p=planes[i];p=Plane{};p.bpp=i==1?8:4;
            D3D12_FEATURE_DATA_FORMAT_SUPPORT support{};support.Format=formats[i];
            hr(device->CheckFeatureSupport(D3D12_FEATURE_FORMAT_SUPPORT,&support,sizeof support),"format support query failed");
            require((support.Support1&D3D12_FORMAT_SUPPORT1_TEXTURE2D) && (support.Support1&D3D12_FORMAT_SUPPORT1_SHADER_LOAD)
                && (support.Support2&D3D12_FORMAT_SUPPORT2_UAV_TYPED_STORE),"NR staging format lacks required GPU support");
            D3D12_RESOURCE_DESC desc{};desc.Dimension=D3D12_RESOURCE_DIMENSION_TEXTURE2D;
            desc.Width=w;desc.Height=h;desc.DepthOrArraySize=1;desc.MipLevels=1;desc.Format=formats[i];desc.SampleDesc.Count=1;
            // Public SL resource allocation guidance requires SRV/UAV-capable textures.
            desc.Flags=D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
            auto properties=heap(D3D12_HEAP_TYPE_DEFAULT);
            hr(device->CreateCommittedResource(&properties,D3D12_HEAP_FLAG_NONE,&desc,D3D12_RESOURCE_STATE_COMMON,nullptr,IID_PPV_ARGS(&p.texture)),"texture allocation failed");
            UINT rows{};UINT64 row_bytes{};device->GetCopyableFootprints(&desc,0,1,0,&p.footprint,&rows,&row_bytes,&p.bytes);
            D3D12_RESOURCE_DESC buffer{};buffer.Dimension=D3D12_RESOURCE_DIMENSION_BUFFER;buffer.Width=p.bytes;
            buffer.Height=1;buffer.DepthOrArraySize=1;buffer.MipLevels=1;buffer.SampleDesc.Count=1;buffer.Layout=D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
            properties=heap(i==3?D3D12_HEAP_TYPE_READBACK:D3D12_HEAP_TYPE_UPLOAD);
            auto& resource=i==3?p.readback:p.upload;
            hr(device->CreateCommittedResource(&properties,D3D12_HEAP_FLAG_NONE,&buffer,
                i==3?D3D12_RESOURCE_STATE_COPY_DEST:D3D12_RESOURCE_STATE_GENERIC_READ,nullptr,IID_PPV_ARGS(&resource)),"staging allocation failed");
            if(i==3) {
                properties=heap(D3D12_HEAP_TYPE_UPLOAD);
                hr(device->CreateCommittedResource(&properties,D3D12_HEAP_FLAG_NONE,&buffer,
                    D3D12_RESOURCE_STATE_GENERIC_READ,nullptr,IID_PPV_ARGS(&p.upload)),"output initialization buffer failed");
            }
        }
    }
    std::vector<uint8_t> run(const Frame& frame,uint32_t& stages,int32_t& result) {
        stages=omsinr::loaded|omsinr::supported;result=-1;last_result=-1;
        try {
            const auto& h=frame.header;frame.validate_planes();create_planes(h.u32(36),h.u32(40));proof.mark(Resources);
            hr(allocator->Reset(),"allocator reset failed");hr(list->Reset(allocator.Get(),nullptr),"list reset failed");
            size_t source=0;
            for(size_t i=0;i<4;++i) {
                auto& p=planes[i];uint8_t* mapped{};D3D12_RANGE no_read{};
                hr(p.upload->Map(0,&no_read,reinterpret_cast<void**>(&mapped)),"upload map failed");
                for(uint32_t y=0;y<height;++y) {
                    auto* row=mapped+p.footprint.Offset+size_t(y)*p.footprint.Footprint.RowPitch;
                    if(i==3)std::memset(row,0xcd,size_t(width)*p.bpp);
                    else std::memcpy(row,frame.planes.data()+source+size_t(y)*width*p.bpp,size_t(width)*p.bpp);
                }
                p.upload->Unmap(0,nullptr);source+=size_t(width)*height*p.bpp;
                auto b=transition(p.texture.Get(),D3D12_RESOURCE_STATE_COMMON,D3D12_RESOURCE_STATE_COPY_DEST);list->ResourceBarrier(1,&b);
                D3D12_TEXTURE_COPY_LOCATION from{},to{};from.pResource=p.upload.Get();from.Type=D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;from.PlacedFootprint=p.footprint;
                to.pResource=p.texture.Get();to.Type=D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
                list->CopyTextureRegion(&to,0,0,0,&from,nullptr);
                b=transition(p.texture.Get(),D3D12_RESOURCE_STATE_COPY_DEST,D3D12_RESOURCE_STATE_COMMON);list->ResourceBarrier(1,&b);
            }
            const bool reset=(h.u32(44)&2) || previous_session!=h.u64(16) || previous_frame+1!=h.u64(24) || generation!=h.u32(32);
            if(backend==Backend::Streamline) {
            const uint32_t index=uint32_t(h.u64(24));sl::FrameToken* frame_token{};
            slcheck(token(frame_token,&index),"frame token");require(frame_token!=nullptr,"null frame token");
            sl::Constants c{};auto matrix=[&](sl::float4x4& m,size_t offset){for(size_t i=0;i<16;++i)reinterpret_cast<float*>(&m)[i]=h.f32(64+(offset+i)*4);};
            matrix(c.cameraViewToClip,0);matrix(c.clipToCameraView,16);matrix(c.clipToPrevClip,32);matrix(c.prevClipToClip,48);
            c.clipToLensClip={};for(int i=0;i<4;++i)reinterpret_cast<float*>(&c.clipToLensClip)[i*4+i]=1;
            auto vector=[&](size_t at){return sl::float3(h.f32(64+at*4),h.f32(68+at*4),h.f32(72+at*4));};
            c.cameraPos=vector(64);c.cameraUp=vector(67);c.cameraRight=vector(70);c.cameraFwd=vector(73);
            c.cameraNear=h.f32(368);c.cameraFar=h.f32(372);c.cameraFOV=h.f32(376);c.cameraAspectRatio=h.f32(380);
            c.jitterOffset=sl::float2(0,0);c.mvecScale=sl::float2(1.0f/width,1.0f/height);
            c.depthInverted=sl::Boolean::eTrue;c.cameraMotionIncluded=sl::Boolean::eTrue;
            c.motionVectors3D=sl::Boolean::eFalse;c.motionVectorsDilated=sl::Boolean::eFalse;c.motionVectorsJittered=sl::Boolean::eFalse;
            c.reset=reset?sl::Boolean::eTrue:sl::Boolean::eFalse;
            slcheck(constants(c,*frame_token,viewport),"frame constants");
            sl::Extent extent{0,0,width,height};std::array<sl::Resource,4> resources;
            for(size_t i=0;i<4;++i) {
                resources[i]=sl::Resource(sl::ResourceType::eTex2d,planes[i].texture.Get(),uint32_t(D3D12_RESOURCE_STATE_COMMON));
                resources[i].width=width;resources[i].height=height;resources[i].nativeFormat=uint32_t(planes[i].footprint.Footprint.Format);
                resources[i].mipLevels=1;resources[i].arrayLayers=1;
            }
            const sl::ResourceTag rt[]={
                {&resources[0],sl::kBufferTypeUpliftInputColor,sl::ResourceLifecycle::eValidUntilEvaluate,&extent},
                {&resources[1],sl::kBufferTypeMotionVectors,sl::ResourceLifecycle::eValidUntilEvaluate,&extent},
                {&resources[2],sl::kBufferTypeDepth,sl::ResourceLifecycle::eValidUntilEvaluate,&extent},
                {&resources[3],sl::kBufferTypeUpliftOutputColor,sl::ResourceLifecycle::eValidUntilEvaluate,&extent}};
            slcheck(tags(*frame_token,viewport,rt,4,reinterpret_cast<sl::CommandBuffer*>(list.Get())),"frame tags");
            if(!feature_initialized) {const auto creation=allocate(reinterpret_cast<sl::CommandBuffer*>(list.Get()),sl::kFeatureDLSS_NR,viewport);proof.feature_creation_code=int32_t(creation);proof.feature_creation_frame=proof.frame;slcheck(creation,"slAllocateResources(NR)");feature_initialized=true;proof.feature_created=true;}
            proof.mark(Initialized);stages|=initialized|attempted;const sl::BaseStructure* inputs[]={&viewport};
            slcheck(evaluate(sl::kFeatureDLSS_NR,*frame_token,inputs,1,reinterpret_cast<sl::CommandBuffer*>(list.Get())),"NR evaluate 1004");
            verify_nr_origin(runtime_root/L"nvngx_dlssnr.dll");if(!proof.steps[Loaded])proof.mark(Loaded);
            } else {
#ifdef OPENOMSI_WITH_NGX
                std::array<ID3D12Resource*,4> native{};
                for(size_t i=0;i<4;++i) {
                    native[i]=planes[i].texture.Get();
                    const auto state=i==3?D3D12_RESOURCE_STATE_UNORDERED_ACCESS:D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
                    auto barrier=transition(native[i],D3D12_RESOURCE_STATE_COMMON,state);list->ResourceBarrier(1,&barrier);
                }
                stages|=attempted;ngx->record(list.Get(),native,width,height,reset,proof,last_result);stages|=initialized;
                for(size_t i=0;i<4;++i) {
                    const auto state=i==3?D3D12_RESOURCE_STATE_UNORDERED_ACCESS:D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
                    auto barrier=transition(native[i],state,D3D12_RESOURCE_STATE_COMMON);list->ResourceBarrier(1,&barrier);
                }
#endif
            }
            // Public Resource::state tells SL to restore COMMON. No aliasing of linear
            // depth, backbuffer, control masks or undocumented NGX parameter keys.
            auto& out=planes[3];auto b=transition(out.texture.Get(),D3D12_RESOURCE_STATE_COMMON,D3D12_RESOURCE_STATE_COPY_SOURCE);list->ResourceBarrier(1,&b);
            D3D12_TEXTURE_COPY_LOCATION from{},to{};from.pResource=out.texture.Get();from.Type=D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
            to.pResource=out.readback.Get();to.Type=D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;to.PlacedFootprint=out.footprint;
            list->CopyTextureRegion(&to,0,0,0,&from,nullptr);
            b=transition(out.texture.Get(),D3D12_RESOURCE_STATE_COPY_SOURCE,D3D12_RESOURCE_STATE_COMMON);list->ResourceBarrier(1,&b);
            hr(list->Close(),"evaluate list close failed");ID3D12CommandList* lists[]={list.Get()};queue->ExecuteCommandLists(1,lists);proof.mark(Submitted);proof.phase="GPU-fence-wait";proof.save_progress();drain();proof.mark(Completed);
            // Present through the proxy exactly once per completed host frame.
            if(swapchain){hr(swapchain->Present(0,0),"Streamline housekeeping Present failed");drain();}
            proof.phase="output-readback";proof.save_progress();
            std::vector<uint8_t> pixels(size_t(width)*height*4);uint8_t* mapped{};D3D12_RANGE range{0,size_t(out.bytes)};
            hr(out.readback->Map(0,&range,reinterpret_cast<void**>(&mapped)),"output map failed");
            for(uint32_t y=0;y<height;++y)std::memcpy(pixels.data()+size_t(y)*width*4,mapped+out.footprint.Offset+size_t(y)*out.footprint.Footprint.RowPitch,size_t(width)*4);
            D3D12_RANGE no_write{};out.readback->Unmap(0,&no_write);
            proof.mark(Read);
            const bool coherent=validate_output(frame,pixels,proof);
            if(coherent) {proof.mark(Validated);stages|=succeeded;result=0;}
            else {proof.error="output inconclusive: unchanged/spatially constant image; not certified as NR";result=-1;}
            proof.result=result;generation=h.u32(32);previous_session=h.u64(16);previous_frame=h.u64(24);
            return pixels;
        }catch(...){result=last_result==0?-1:last_result;throw;}
    }
    ~Impl() {
        try {if(queue && fence && event.h!=INVALID_HANDLE_VALUE)drain();}
        catch(...) {ExitProcess(3);}
#ifdef OPENOMSI_WITH_NGX
        ngx.reset();
#endif
        if(feature_initialized && free_resources)free_resources(sl::kFeatureDLSS_NR,viewport);
        if(sl_initialized && shutdown)shutdown();
        swapchain.Reset();queue.Reset();proxy_factory.Reset();proxy_device.Reset();
        for(auto& p:planes)p=Plane{};
        list.Reset();allocator.Reset();fence.Reset();device.Reset();factory.Reset();
        if(window)DestroyWindow(window);if(module)FreeLibrary(module);
        if(dll_directory)RemoveDllDirectory(dll_directory);
    }
};
NeuralHost::NeuralHost(const std::filesystem::path& runtime,const std::string& project,Backend backend,Evidence& proof,const std::filesystem::path& data)
    :impl(std::make_unique<Impl>(runtime,project,backend,proof,data)){proof.initialization_calls=proof.calls.size();}
bool ngx_compiled() {
#ifdef OPENOMSI_WITH_NGX
    return true;
#else
    return false;
#endif
}
NeuralHost::~NeuralHost()=default;
std::vector<uint8_t> NeuralHost::evaluate(const Frame& f,uint32_t& stages,int32_t& result){return impl->run(f,stages,result);}
}
