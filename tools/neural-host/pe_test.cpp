#include "pe.hpp"
#include "evidence.hpp"
#include <sstream>
#include <iostream>
using namespace omsinr;
int main(int argc,char** argv) {
    try {
        std::string bytes(1024,'\0');
        auto set16=[&](size_t i,uint16_t v){bytes[i]=char(v);bytes[i+1]=char(v>>8);};
        auto set32=[&](size_t i,uint32_t v){for(size_t k=0;k<4;++k)bytes[i+k]=char(v>>(k*8));};
        set16(0,0x5a4d);set32(0x3c,0x80);set32(0x80,0x4550);set16(0x84,0x8664);
        set16(0x86,1);set16(0x94,240);set16(0x96,0x2000);set16(0x98,0x20b);
        set32(0x98+60,0x200);set32(0x98+108,16);
        set32(0x98+120,0x1000);set32(0x98+124,40);
        const size_t section=0x98+240;set32(section+12,0x1000);set32(section+16,512);set32(section+20,512);
        set32(512+12,0x1080);bytes.replace(640,12,"kernel32.dll");
        auto inspect=[&]{std::istringstream input(bytes);return PeImports(input,bytes.size()).inspect();};
        require(inspect().size()==1 && inspect()[0].name=="kernel32.dll","import roundtrip failed");
        auto rejects=[&]{bool rejected=false;try{inspect();}catch(const std::exception&){rejected=true;}require(rejected,"malformed PE accepted");};
        set16(0x84,0x14c);rejects();set16(0x84,0x8664);
        set16(0x98,0x10b);rejects();set16(0x98,0x20b);
        set32(512+12,0xffffffff);rejects();set32(512+12,0x1080);
        bytes[640]='/';rejects();bytes[640]='k';
        const auto complete=bytes;bytes.resize(700);rejects();bytes=complete;
        set32(0x98+112+13*8,0x1100);set32(0x98+116+13*8,64);
        set32(0x300,1);set32(0x304,0x1150);bytes.replace(0x350,11,"nvapi64.dll");
        const auto imports=inspect();require(imports.size()==2 && imports[1].delayed,"delay imports lost");
        set32(0x300,0);rejects();
        Frame f;f.header.set32(36,2);f.header.set32(40,2);f.planes.resize(68);
        std::vector<uint8_t> constant;
        for(int i=0;i<4;++i)constant.insert(constant.end(),{20,30,40,255});
        Evidence proof;require(!validate_output(f,constant,proof),"spatially constant colored output certified");
        Evidence trace;trace.call("CPU fixture","initialization receipt",7);trace.initialization_calls=1;
        trace.next_frame(1);trace.call("CPU fixture","frame receipt",9);trace.next_frame(2);
        require(trace.calls.size()==1 && trace.calls[0].code==7,"frame traces lost initialization or grew across frames");
        require(!trace.steps[Loaded] && !trace.steps[Validated],"CPU trace became NVIDIA evidence");
        if(argc==2) {
            std::ifstream input(argv[1],std::ios::binary|std::ios::ate);const auto size=input.tellg();
            require(size>0,"host PE missing");const auto actual=PeImports(input,uint64_t(size)).inspect(false);
            require(!actual.empty(),"host imports missing");
            std::cout<<"Built host PE imports: "<<actual.size()<<'\n';
        }
        std::cout<<"PE bounds/import/delay-import and output validation tests passed; no DLL loaded.\n";
    }catch(const std::exception& e) {std::cerr<<e.what()<<'\n';return 1;}
}
