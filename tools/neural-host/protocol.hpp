#pragma once
// openOMSI's own staging ABI. No proprietary structures or Feature-18 constants.
#include <array>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <stdexcept>
#include <vector>

namespace omsinr {
constexpr size_t header_size=384;
constexpr uint64_t max_pixels=3840*2160;
constexpr uint32_t loaded=1,supported=2,initialized=4,attempted=8,succeeded=16;
inline void require(bool condition,const char* message) {if(!condition)throw std::runtime_error(message);}
struct Header {
    std::array<uint8_t,header_size> bytes{};
    uint32_t u32(size_t i) const {return uint32_t(bytes[i])|(uint32_t(bytes[i+1])<<8)|(uint32_t(bytes[i+2])<<16)|(uint32_t(bytes[i+3])<<24);}
    uint64_t u64(size_t i) const {return u32(i)|(uint64_t(u32(i+4))<<32);}
    void set32(size_t i,uint32_t v){for(int k=0;k<4;++k)bytes[i+k]=uint8_t(v>>(k*8));}
    void set64(size_t i,uint64_t v){set32(i,uint32_t(v));set32(i+4,uint32_t(v>>32));}
    float f32(size_t i) const {const auto b=u32(i);float f;std::memcpy(&f,&b,4);return f;}
    uint64_t pixels() const {return uint64_t(u32(36))*u32(40);}
    void validate() const {
        require(std::memcmp(bytes.data(),"OMSINR01",8)==0 && u32(8)==1 && u32(12)==1,"bad request protocol");
        require(u32(36)>0 && u32(40)>0 && pixels()<=max_pixels,"bad extent");
        require((u32(44)&~7u)==0 && (u32(44)&5u)==5u,"requires reverse Z and encoded sRGB LDR");
        require(u64(48)==pixels()*17,"bad plane byte count");
        for(size_t i=64;i<header_size;i+=4)require(std::isfinite(f32(i)),"nonfinite camera");
    }
    void read(std::istream& f){f.read(reinterpret_cast<char*>(bytes.data()),bytes.size());require(bool(f),"truncated header");validate();}
    void write(std::ostream& f) const {f.write(reinterpret_cast<const char*>(bytes.data()),bytes.size());require(bool(f),"write header failed");}
};
struct Frame {
    Header header;
    std::vector<uint8_t> planes;
    void validate_planes() const {
        header.validate();const auto n=size_t(header.pixels());
        require(planes.size()==n*17,"wrong plane size");
        for(size_t i=n*4;i<n*16;i+=4) {
            uint32_t bits=uint32_t(planes[i])|(uint32_t(planes[i+1])<<8)|(uint32_t(planes[i+2])<<16)|(uint32_t(planes[i+3])<<24);
            float value{};std::memcpy(&value,&bits,4);
            require(std::isfinite(value),"nonfinite guide");
            if(i>=n*12)require(value>=0 && value<=1,"depth outside reverse Z [0,1]");
        }
        require(header.f32(368)>0 && header.f32(372)>header.f32(368),"invalid camera near/far");
        require(header.f32(376)>0 && header.f32(376)<3.142f && header.f32(380)>0,"invalid camera FOV/aspect");
        // Row-major projection and inverse must agree. This also rejects zero matrices.
        for(size_t row=0;row<4;++row)for(size_t col=0;col<4;++col) {
            float product=0;
            for(size_t k=0;k<4;++k)product+=header.f32(64+(row*4+k)*4)*header.f32(128+(k*4+col)*4);
            require(std::abs(product-(row==col?1.f:0.f))<0.005f,"projection inverse mismatch");
        }
    }
    void read(std::istream& f) {
        header.read(f);planes.resize(size_t(header.u64(48)));
        f.read(reinterpret_cast<char*>(planes.data()),planes.size());require(bool(f),"truncated planes");
        require(f.peek()==std::char_traits<char>::eof(),"trailing input");
    }
};
}
