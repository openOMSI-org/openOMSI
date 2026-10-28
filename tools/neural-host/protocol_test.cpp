#include "protocol.hpp"
#include "evidence.hpp"
#include <iostream>
#include <sstream>
int main(int argc,char**argv) {
    try {
        if(argc==2) {
            std::ifstream input(argv[1],std::ios::binary);omsinr::Frame f;f.read(input);
            omsinr::require(f.header.u64(16)==13 && f.header.u64(24)==6,"cross language identity");
            std::cout<<"Rust request parsed by C++: "<<f.header.pixels()<<" pixels\n";
            return 0;
        }
        omsinr::Header h;std::memcpy(h.bytes.data(),"OMSINR01",8);
        h.set32(8,1);h.set32(12,1);h.set32(36,2);h.set32(40,3);h.set32(44,7);h.set64(48,102);
        h.validate();
        auto setfloat=[&](size_t at,float value){uint32_t bits{};std::memcpy(&bits,&value,4);h.set32(at,bits);};
        for(size_t i=0;i<4;++i) {setfloat(64+(i*4+i)*4,1);setfloat(128+(i*4+i)*4,1);}
        setfloat(368,0.1f);setfloat(372,1000);setfloat(376,1);setfloat(380,2.f/3.f);
        omsinr::Frame frame;frame.header=h;frame.planes.resize(102);frame.validate_planes();
        auto rejects=[&](auto action){bool refused=false;try{action();}catch(const std::exception&){refused=true;}omsinr::require(refused,"malformed guide accepted");};
        frame.planes[72+3]=0x40; // depth 2.0, beyond reverse-Z range
        rejects([&]{frame.validate_planes();});frame.planes[75]=0;
        frame.planes[24+2]=0xc0;frame.planes[24+3]=0x7f; // NaN motion
        rejects([&]{frame.validate_planes();});frame.planes[26]=frame.planes[27]=0;
        std::vector<uint8_t> output(24,0);omsinr::Evidence proof;
        omsinr::require(!omsinr::validate_output(frame,output,proof),"identical image certified");
        output.assign(24,0xcd);proof={};omsinr::require(!omsinr::validate_output(frame,output,proof),"unwritten sentinel certified");
        output.assign(24,0);output[0]=20;output[1]=30;proof={};
        omsinr::require(omsinr::validate_output(frame,output,proof),"transformed nonconstant fixture rejected");
        omsinr::require(!proof.steps[omsinr::Loaded] && !proof.steps[omsinr::Validated],"pixel validation invented runtime evidence");
        h.set32(36,0xffffffff);bool refused=false;
        try{h.validate();}catch(const std::exception&){refused=true;}
        omsinr::require(refused,"oversized frame accepted");
        std::cout<<"protocol validation passed (no inference)\n";
    }catch(const std::exception&e){std::cerr<<e.what()<<'\n';return 1;}
}
