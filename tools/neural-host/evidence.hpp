#pragma once
#include "protocol.hpp"
#include <filesystem>
#include <string>
#include <ostream>
#include <iomanip>
#include <algorithm>
#include <iostream>
namespace omsinr {
enum Step { Found, Verified, Loaded, Supported, Initialized, Resources, Submitted, Completed, Read, Validated, StepCount };
inline constexpr const char* step_names[] = {"RUNTIME FOUND","RUNTIME VERIFIED","RUNTIME LOADED","GPU SUPPORTED","FEATURE INITIALIZED","RESOURCES ALLOCATED","EVALUATION SUBMITTED","GPU COMPLETED","OUTPUT READ","OUTPUT VALIDATED"};
inline std::string json_string(const std::string& value) {
    std::string out="\"";
    for(unsigned char c:value) {
        if(c=='"' || c=='\\') {out+='\\';out+=char(c);}
        else if(c<32) {const char* hex="0123456789abcdef";out+="\\u00";out+=hex[c>>4];out+=hex[c&15];}
        else out+=char(c);
    }
    return out+'"';
}
struct Evidence {
    struct Call {std::string domain,operation;int64_t code;};
    std::vector<Call> calls;
    size_t initialization_calls{};
    std::string gpu_name,driver_version,luid;
    uint32_t gpu_vendor{},gpu_device{};
    uint64_t vram_bytes{};
    int64_t support_mask=-1;
    int64_t feature_creation_code=-1;
    uint64_t feature_creation_frame{};
    bool feature_created=false;
    std::string phase="preflight";
    std::filesystem::path progress_report;
    std::array<bool,StepCount> steps{};
    std::string backend="unselected",error,input_sha256,output_sha256,runtime_directory,runtime_sha256,runtime_version;
    bool core_initialized=false,native_windows=false;
    int32_t api_result=-1;
    int32_t result=-1;
    uint64_t frame{},changed_pixels{},valid_pixels{};
    double cpu_elapsed_ms{};
    void mark(Step step) {steps[step]=true;std::cout<<step_names[step]<<std::endl;save_progress();}
    void next_frame(uint64_t number) {
        calls.resize(initialization_calls);phase="frame-validation";
        for(size_t i=Initialized;i<StepCount;++i)steps[i]=false;
        error.clear();input_sha256.clear();output_sha256.clear();frame=number;
        changed_pixels=valid_pixels=0;result=-1;cpu_elapsed_ms=0;
        save_progress();
    }
    void call(const char* domain,const char* operation,int64_t code) {
        calls.push_back({domain,operation,code});api_result=static_cast<int32_t>(code);
        std::cout<<domain<<" "<<operation<<" result="<<code<<std::endl;save_progress();
    }
    void write(std::ostream& out) const {
        out<<"{\"schema\":1,\"backend\":"<<json_string(backend)<<",\"frame\":"<<frame
           <<",\"result\":"<<result<<",\"error\":"<<json_string(error)<<",\"stages\":{";
        for(size_t i=0;i<StepCount;++i)out<<(i?",":"")<<json_string(step_names[i])<<":"<<(steps[i]?"true":"false");
        out<<"},\"phase\":"<<json_string(phase)<<",\"gpu\":{\"name\":"<<json_string(gpu_name)
           <<",\"vendor\":"<<gpu_vendor<<",\"device\":"<<gpu_device<<",\"luid\":"<<json_string(luid)
           <<",\"driver_version\":"<<json_string(driver_version)<<",\"dedicated_vram_bytes\":"<<vram_bytes
           <<"},\"support_mask\":"<<support_mask<<",\"feature_creation\":{\"code\":"<<feature_creation_code
           <<",\"frame\":"<<feature_creation_frame<<",\"created\":"<<(feature_created?"true":"false")
           <<",\"reused\":"<<((feature_created && feature_creation_frame!=frame)?"true":"false")<<"},\"api_calls\":[";
        for(size_t i=0;i<calls.size();++i) {
            const auto& c=calls[i];out<<(i?",":"")<<"{\"domain\":"<<json_string(c.domain)
                <<",\"operation\":"<<json_string(c.operation)<<",\"code\":"<<c.code<<"}";
        }
        out<<"],\"runtime_directory\":"<<json_string(runtime_directory)<<",\"runtime_sha256\":"<<json_string(runtime_sha256)
           <<",\"runtime_version\":"<<json_string(runtime_version)<<",\"core_initialized\":"<<(core_initialized?"true":"false")
           <<",\"native_windows\":"<<(native_windows?"true":"false")<<",\"last_api_result\":"<<api_result
           <<",\"input_sha256\":"<<json_string(input_sha256)<<",\"output_sha256\":"<<json_string(output_sha256)
           <<",\"changed_valid_pixels\":"<<changed_pixels<<",\"valid_pixels\":"<<valid_pixels
           <<",\"cpu_elapsed_ms\":"<<std::setprecision(10)<<cpu_elapsed_ms
           <<",\"quality_validated\":false,\"presented_in_game\":false}\n";
    }
    void save_progress() const {
        if(progress_report.empty())return;
        std::ofstream out(progress_report,std::ios::binary|std::ios::trunc);write(out);
        out.flush();require(bool(out),"progress report write failed");
    }
};
// Basic evidence of a transformed image, not perceptual qualification of a model.
inline bool validate_output(const Frame& frame,const std::vector<uint8_t>& output,Evidence& proof) {
    const auto n=size_t(frame.header.pixels());require(output.size()==n*4,"wrong output extent");
    bool spatial_variation=false;uint64_t changed_total=0;
    for(size_t i=0;i<n;++i) {
        const bool valid=frame.planes[n*16+i]==255;
        proof.valid_pixels+=valid;bool changed=false;
        for(size_t c=0;c<3;++c) {
            const auto v=output[i*4+c];spatial_variation|=v!=output[c];
            changed|=v!=frame.planes[i*4+c];
        }
        proof.changed_pixels+=changed && valid;changed_total+=changed;
    }
    // An unchanged or spatially constant RGB image is inconclusive. Engine validity
    // is applied separately during composition, including the reset frame.
    return changed_total>0 && spatial_variation;
}
}
