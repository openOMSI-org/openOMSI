#pragma once
// Bounded, read-only import inspection. Does not load or execute the file.
#include "protocol.hpp"
#include <string>
#include <algorithm>
namespace omsinr {
struct Import {std::string name;bool delayed=false;};
class PeImports {
    std::istream& input;uint64_t length;
    struct Section {uint32_t rva,raw_size,raw;};
    std::vector<Section> sections;
    uint32_t headers{};
    std::vector<uint8_t> read(uint64_t offset,size_t size) {
        require(offset<=length && size<=length-offset,"PE read outside file");
        std::vector<uint8_t> bytes(size);input.clear();input.seekg(std::streamoff(offset));
        input.read(reinterpret_cast<char*>(bytes.data()),std::streamsize(size));
        require(bool(input),"PE read failed");return bytes;
    }
    uint32_t u32(uint64_t offset) {const auto b=read(offset,4);return b[0]|uint32_t(b[1])<<8|uint32_t(b[2])<<16|uint32_t(b[3])<<24;}
    uint16_t u16(uint64_t offset) {const auto b=read(offset,2);return uint16_t(b[0]|uint16_t(b[1])<<8);}
    uint64_t file_offset(uint32_t rva,size_t size) {
        if(rva<headers) {require(size<=uint64_t(headers)-rva,"PE header RVA overflow");return rva;}
        uint64_t found=0;bool matched=false;
        for(const auto& s:sections)if(rva>=s.rva && uint64_t(rva)-s.rva<s.raw_size) {
            require(!matched && size<=uint64_t(s.raw_size)-(rva-s.rva),"ambiguous/truncated PE RVA");
            found=uint64_t(s.raw)+(rva-s.rva);matched=true;
        }
        require(matched,"unmapped PE RVA");return found;
    }
    std::string name(uint32_t rva) {
        std::string out;
        for(uint32_t i=0;i<260;++i) {
            require(uint64_t(rva)+i<=0xffffffffu,"PE name overflow");
            const auto c=read(file_offset(rva+i,1),1)[0];
            if(!c) {require(!out.empty(),"empty import name");return out;}
            require((c>='a'&&c<='z') || (c>='A'&&c<='Z') || (c>='0'&&c<='9') || c=='.' || c=='_' || c=='-',"unsafe import name");out+=char(c);
        }
        throw std::runtime_error("unterminated PE import name");
    }
public:
    PeImports(std::istream& stream,uint64_t size):input(stream),length(size) {}
    std::vector<Import> inspect(bool dll=true) {
        require(length>=64 && length<=uint64_t(2)*1024*1024*1024 && u16(0)==0x5a4d,"invalid/bounded PE DOS header");
        const auto nt=u32(0x3c);require(nt>=64 && nt<16*1024*1024 && u32(nt)==0x4550,"invalid PE signature");
        require(u16(nt+4)==0x8664 && (!dll || (u16(nt+22)&0x2000)),"requires PE AMD64 DLL");
        const auto count=u16(nt+6),optional_size=u16(nt+20);const uint64_t optional=nt+24;
        require(count>0 && count<=96 && optional_size>=128 && optional_size<=4096 && u16(optional)==0x20b,"invalid PE32+ headers");
        headers=u32(optional+60);require(headers<=length,"PE SizeOfHeaders outside file");
        for(uint32_t i=0;i<count;++i) {
            const auto at=optional+optional_size+i*40;const auto b=read(at,40);(void)b;
            Section s{u32(at+12),u32(at+16),u32(at+20)};
            require(uint64_t(s.raw)+s.raw_size<=length,"PE section outside file");sections.push_back(s);
        }
        const auto directories=u32(optional+108);require(directories<=16 && 112+directories*8<=optional_size,"invalid PE directories");
        std::vector<Import> imports;
        for(bool delayed:{false,true}) {
            const unsigned index=delayed?13:1;if(directories<=index)continue;
            const auto rva=u32(optional+112+index*8),bytes=u32(optional+116+index*8);if(!rva && !bytes)continue;
            const uint32_t stride=delayed?32:20;require(rva && bytes>=stride && bytes/stride<=4096,"invalid PE import table size");
            bool terminated=false;
            for(uint32_t i=0;i<bytes/stride;++i) {
                require(uint64_t(rva)+uint64_t(i)*stride<=0xffffffffu,"PE import RVA overflow");
                const auto at=file_offset(rva+i*stride,stride);const auto descriptor=read(at,stride);
                if(std::all_of(descriptor.begin(),descriptor.end(),[](auto c){return c==0;})) {terminated=true;break;}
                if(delayed)require(u32(at)==1,"unsupported delay-import VA addressing");
                imports.push_back({name(u32(at+(delayed?4:12))),delayed});
            }
            require(terminated,"unterminated PE import table");
        }
        return imports;
    }
};
}
