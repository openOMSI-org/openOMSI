#pragma once
#include "streamline.hpp"
#include "runtime_files.hpp"
#include <chrono>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <thread>
namespace omsinr {
inline void serve(NeuralHost& runtime,const std::filesystem::path& session_path,Evidence& proof) {
    namespace fs=std::filesystem;
        const auto session=fs::canonical(session_path);
        require(fs::is_directory(session) && !fs::exists(session/"host.ready"),"use a fresh private session directory");
        for(const auto& e:fs::directory_iterator(session))
            require(e.path().extension()!=".request","stale requests in directory");
        struct Ready {fs::path path;~Ready(){std::error_code ec;fs::remove(path,ec);}} ready{session/"host.ready"};
        {std::ofstream f(ready.path,std::ios::binary);f<<"openomsi-streamline-host/1\n";require(bool(f),"ready write failed");}
        std::cout<<"Ready for transport; no neural frame has been evaluated. Create host.stop to exit.\n";
        while(!fs::exists(session/"host.stop")) {
            for(const auto& e:fs::directory_iterator(session)) {
                if(e.path().extension()!=".request" || !e.is_regular_file() || e.is_symlink())continue;
                const auto stem=e.path().stem().string();
                require(stem.size()==33 && stem[16]=='-' && stem.find_first_not_of("0123456789abcdef-")==std::string::npos,"bad request name");
                std::ifstream input(e.path(),std::ios::binary);if(!input)continue;
                Frame frame;frame.read(input);input.close();
                std::ostringstream identity;identity<<std::hex<<std::setfill('0')<<std::setw(16)<<frame.header.u64(16)<<'-'<<std::setw(16)<<frame.header.u64(24);
                require(identity.str()==stem,"request name does not match frame identity");
                proof.next_frame(frame.header.u64(24));
                proof.input_sha256=RuntimeFiles::sha256_parts({{frame.header.bytes.data(),header_size},{frame.planes.data(),frame.planes.size()}});
                const auto start=std::chrono::steady_clock::now();
                uint32_t stages=0;int32_t result=-1;std::vector<uint8_t> output;
                try {output=runtime.evaluate(frame,stages,result);}
                catch(const std::exception& ex){proof.error=ex.what();std::cerr<<"frame failed: "<<ex.what()<<'\n';}
                proof.result=result;
                proof.cpu_elapsed_ms=std::chrono::duration<double,std::milli>(std::chrono::steady_clock::now()-start).count();
                if(!output.empty())proof.output_sha256=RuntimeFiles::sha256_parts({{output.data(),output.size()}});
                proof.save_progress();
                proof.write(std::cout);
                Header response=frame.header;response.set32(12,2);response.set64(48,frame.header.pixels()*4);
                response.set32(56,stages);response.set32(60,uint32_t(result));
                const auto pending=session/(stem+".hostpending"),out=session/(stem+".response");
                {std::ofstream f(pending,std::ios::binary|std::ios::trunc);response.write(f);
                    f.write(reinterpret_cast<const char*>(output.data()),output.size());require(bool(f),"response write failed");}
                // A producer that timed out deletes its request. Do not publish late results.
                if(fs::exists(e.path())) {require(!fs::exists(out),"output already exists");fs::rename(pending,out);}
                else fs::remove(pending);
                std::error_code ec;fs::remove(e.path(),ec);
                std::cout<<"frame="<<frame.header.u64(24)<<" stages="<<stages<<" result="<<result
                    <<" output_bytes="<<output.size()<<std::endl;
                if(result!=0 || stages!=31)throw std::runtime_error("host evaluation failed; original game rendering retained");
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(2));
        }

}
}
