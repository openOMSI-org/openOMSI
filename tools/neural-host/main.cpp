// Portable modes inspect inputs only; none simulates NVIDIA.
#include "protocol.hpp"
#include "evidence.hpp"
#include <filesystem>
#include <iostream>
#include <map>
#include <algorithm>
#include <chrono>
#ifdef _WIN32
#include "diagnostics.hpp"
#include "serve.hpp"
#endif
namespace fs=std::filesystem;
using namespace omsinr;
namespace {
Frame read_frame(const fs::path& path) {
    require(fs::is_regular_file(path) && !fs::is_symlink(path),"capture must be a regular file");
    require(fs::file_size(path)<=header_size+max_pixels*17,"oversized capture");
    std::ifstream file(path,std::ios::binary);Frame frame;frame.read(file);frame.validate_planes();return frame;
}
std::vector<fs::path> sequence(const fs::path& path) {
    std::vector<fs::path> frames;
    if(fs::is_directory(path)) {
        for(const auto& item:fs::directory_iterator(path))if(item.path().extension()==".omsi-nr")frames.push_back(item.path());
        std::sort(frames.begin(),frames.end());
    } else frames.push_back(path);
    require(!frames.empty() && frames.size()<=16,"replay requires 1..16 captures in sequence order");
    uint64_t session=0,previous=0;
    for(const auto& file:frames) {
        const auto f=read_frame(file);
        require(!previous || (session==f.header.u64(16) && previous+1==f.header.u64(24)),"capture sequence has a gap or mixed sessions");
        session=f.header.u64(16);previous=f.header.u64(24);
    }
    return frames;
}
void self_test() {
    Header h;std::memcpy(h.bytes.data(),"OMSINR01",8);h.set32(8,1);h.set32(12,1);
    h.set32(36,2);h.set32(40,2);h.set32(44,7);h.set64(48,68);h.validate();
    h.set32(40,0);bool rejected=false;try {h.validate();}catch(...){rejected=true;}
    require(rejected,"self test accepted invalid dimensions");
    Evidence e;require(!e.steps[Validated] && !e.steps[Loaded],"invalid evidence defaults");
    require(json_string("\"\\\n")=="\"\\\"\\\\\\u000a\"","invalid JSON escaping");
#ifdef _WIN32
    const uint8_t abc[]={'a','b','c'};
    require(RuntimeFiles::sha256_parts({{abc,1},{abc+1,2}})==
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "SHA256 evidence hash mismatch");
#endif
    std::cout<<"Self-test passed: parser and evidence; no NVIDIA runtime loaded or evaluated.\n";
}
#ifdef _WIN32
fs::path output_file(const fs::path& base,const char* suffix) {auto path=base;path+=suffix;return path;}
void save_output(const fs::path& base,const Frame& f,const std::vector<uint8_t>& pixels) {
    {std::ofstream raw(output_file(base,".rgba"),std::ios::binary);raw.write(reinterpret_cast<const char*>(pixels.data()),pixels.size());require(bool(raw),"raw output write failed");}
    BITMAPFILEHEADER file{};BITMAPINFOHEADER info{};
    file.bfType=0x4d42;file.bfOffBits=sizeof(file)+sizeof(info);file.bfSize=DWORD(file.bfOffBits+pixels.size());
    info.biSize=sizeof(info);info.biWidth=LONG(f.header.u32(36));info.biHeight=-LONG(f.header.u32(40));
    info.biPlanes=1;info.biBitCount=32;info.biCompression=BI_RGB;info.biSizeImage=DWORD(pixels.size());
    std::ofstream image(output_file(base,".bmp"),std::ios::binary);
    image.write(reinterpret_cast<char*>(&file),sizeof(file));image.write(reinterpret_cast<char*>(&info),sizeof(info));
    auto bgra=pixels;for(size_t i=0;i<bgra.size();i+=4)std::swap(bgra[i],bgra[i+2]);
    image.write(reinterpret_cast<char*>(bgra.data()),bgra.size());require(bool(image),"BMP output write failed");
}
Backend choose_backend(const fs::path& runtime,const std::string& choice,Evidence& proof,bool verification_only=false) {
    bool sl=true;for(const auto* name:{"sl.interposer.dll","sl.common.dll","sl.dlss_nr.dll","nvngx_dlssnr.dll"})sl&=fs::is_regular_file(runtime/name);
    const bool nr=fs::is_regular_file(runtime/"nvngx_dlssnr.dll");
    require(choice=="auto" || choice=="streamline" || choice=="ngx","backend must be auto, streamline or ngx");
    if(choice=="streamline" || (choice=="auto" && sl)) {
        proof.backend="streamline-nr-1004";require(sl,"Streamline NR requires sl.interposer.dll, sl.common.dll, sl.dlss_nr.dll and nvngx_dlssnr.dll");
        proof.mark(Found);return Backend::Streamline;
    }
    if(choice=="ngx" || (choice=="auto" && (ngx_compiled() || verification_only) && nr)) {
        proof.backend="ngx-reserved18-experimental";
        require(verification_only || ngx_compiled(),"NGX not compiled; build native MSVC with OPENOMSI_ENABLE_NGX=ON");
        require(nr,"MissingRuntime: nvngx_dlssnr.dll is absent (NGX does not require sl.dlss_nr.dll)");
        proof.mark(Found);return Backend::Ngx;
    }
    throw std::runtime_error("MissingRuntime: neither compiled backend has its required NR files; use --diagnose");
}
#endif
}
int run_host(int argc,char** argv) {
    Evidence proof;fs::path report;
    try {
        require(argc>=2,"Use --self-test, --inspect-frame <capture>, --diagnose [--runtime <dir>], --validate-runtime, --replay-frame <capture-or-dir>, or --serve. See docs/DLSS5_RUNTIME_VALIDATION.md.");
        const std::string mode=argv[1];std::map<std::string,std::string> options;
        fs::path capture;int at=2;
        if(mode=="--replay-frame" || mode=="--inspect-frame") {require(argc>=3,"capture path required");capture=fs::u8path(argv[2]);at=3;}
        for(;at<argc;++at) {
            const std::string key=argv[at];require(!options.count(key),"duplicate option");
            if(key=="--accept-runtime-license-and-experimental-ldr" || key=="--accept-experimental-ngx18")options[key]="1";
            else {require(key=="--runtime" || key=="--project" || key=="--backend" || key=="--output" || key=="--session","unknown option");
                require(at+1<argc,"option value missing");options[key]=argv[++at];}
        }
        if(mode=="--self-test") {require(options.empty(),"self-test takes no options");self_test();return 0;}
        if(mode=="--build-info") {
            require(options.empty(),"build-info takes no options");
#ifdef _WIN32
            std::cout<<"{\"windows_x64\":true,\"ngx_compiled\":"<<(ngx_compiled()?"true":"false");
#else
            std::cout<<"{\"windows_x64\":false,\"ngx_compiled\":false";
#endif
#ifdef _MSC_VER
            std::cout<<",\"msvc\":"<<_MSC_VER;
#else
            std::cout<<",\"msvc\":0";
#endif
            std::cout<<",\"neural_evaluation\":false}\n";return 0;
        }
        if(mode=="--inspect-frame") {
            for(const auto& path:sequence(capture)) {
                const auto f=read_frame(path);std::cout<<path.u8string()<<" frame="<<f.header.u64(24)
                    <<" extent="<<f.header.u32(36)<<'x'<<f.header.u32(40)<<" generation="<<f.header.u32(32)
                    <<" reset="<<bool(f.header.u32(44)&2)<<" bytes="<<f.planes.size()<<" inputs valid; no inference\n";
            }
            return 0;
        }
#ifdef _WIN32
        const auto runtime=options.count("--runtime")?fs::absolute(fs::u8path(options.at("--runtime"))):fs::current_path();
        proof.native_windows=GetProcAddress(GetModuleHandleW(L"ntdll.dll"),"wine_get_version")==nullptr;
        if(mode=="--diagnose") {diagnose(runtime);return 0;}
        require(mode=="--verify-runtime" || mode=="--validate-runtime" || mode=="--replay-frame" || mode=="--serve","unknown mode");
        require(options.count("--output"),"provide --output <new-directory> for reports and NGX logs");
        const auto output=fs::absolute(fs::u8path(options.at("--output")));
        require(fs::create_directory(output),"output directory already exists; use a new one");report=output/"report.json";
        proof.progress_report=report;proof.save_progress();
        std::vector<fs::path> inputs;if(mode=="--replay-frame")inputs=sequence(capture);
        if(mode=="--verify-runtime") {
            const auto backend=choose_backend(runtime,options.count("--backend")?options.at("--backend"):"ngx",proof,true);
            proof.runtime_directory=runtime.u8string();proof.phase="signature-and-dependencies";
            RuntimeFiles verified(fs::canonical(runtime),backend==Backend::Ngx);
            proof.runtime_sha256=verified.digests.at(L"nvngx_dlssnr.dll");proof.runtime_version=file_version(runtime/"nvngx_dlssnr.dll");
            if(backend==Backend::Ngx)check_ngx_profile(runtime/"nvngx_dlssnr.dll");
            proof.mark(Verified);proof.save_progress();proof.write(std::cout);return 0;
        }
        require(options.count("--accept-runtime-license-and-experimental-ldr"),"runtime use requires explicit license and unvalidated LDR input acknowledgement");
        require(options.count("--project"),"provide your own project GUID with --project");
        const auto backend=choose_backend(runtime,options.count("--backend")?options.at("--backend"):"auto",proof);
        proof.runtime_directory=runtime.u8string();
        if(backend==Backend::Ngx)require(options.count("--accept-experimental-ngx18"),"NGX Feature18 private parameter profile requires --accept-experimental-ngx18");
        proof.phase="signature-and-dependencies";proof.save_progress();
        NeuralHost host(fs::canonical(runtime),options.at("--project"),backend,proof,output);
        if(mode=="--serve") {require(options.count("--session"),"provide --session <fresh-private-directory>");serve(host,fs::u8path(options.at("--session")),proof);}
        for(const auto& path:inputs) {
            const auto frame=read_frame(path);proof.next_frame(frame.header.u64(24));
            proof.input_sha256=RuntimeFiles::sha256_parts({{frame.header.bytes.data(),header_size},{frame.planes.data(),frame.planes.size()}});
            const auto start=std::chrono::steady_clock::now();uint32_t stages=0;int32_t result=-1;
            std::vector<uint8_t> pixels;
            try {pixels=host.evaluate(frame,stages,result);}catch(...) {proof.result=result;throw;}
            proof.cpu_elapsed_ms=std::chrono::duration<double,std::milli>(std::chrono::steady_clock::now()-start).count();
            const bool valid_output=stages==31 && result==0 && proof.steps[Validated];
            auto base=output/path.stem();if(!valid_output)base+=".unvalidated";
            save_output(base,frame,pixels);
            proof.output_sha256=RuntimeFiles::sha256_parts({{pixels.data(),pixels.size()}});
            {std::ofstream f(output_file(base,".json"));proof.write(f);require(bool(f),"frame report write failed");}
            proof.write(std::cout);
            if(!valid_output)throw std::runtime_error(proof.error.empty()?"evaluation did not produce qualified output":proof.error);
        }
        {std::ofstream f(report);proof.write(f);require(bool(f),"report write failed");}
        if(mode=="--validate-runtime")std::cout<<"Runtime initialized and support queried; no feature/image evaluated.\n";
        return 0;
#else
        throw std::runtime_error("NR runtime modes require Windows x64. This binary only validates capture inputs and self-tests.");
#endif
    } catch(const std::exception& error) {
        proof.error=error.what();std::cerr<<"Failed: "<<error.what()<<'\n';proof.write(std::cout);
        if(!report.empty()) {std::ofstream f(report);proof.write(f);}
        return 1;
    }
}
#ifdef _WIN32
int wmain(int argc,wchar_t** argv) {
    std::vector<std::string> strings;std::vector<char*> arguments;
    for(int i=0;i<argc;++i) {
        const int size=WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,argv[i],-1,nullptr,0,nullptr,nullptr);
        if(size<=0)return 1;std::string value(size,'\0');
        WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,argv[i],-1,value.data(),size,nullptr,nullptr);
        value.pop_back();strings.push_back(std::move(value));
    }
    for(auto& value:strings)arguments.push_back(value.data());
    SetConsoleOutputCP(CP_UTF8);return run_host(argc,arguments.data());
}
#else
int main(int argc,char** argv) {return run_host(argc,argv);}
#endif
