#pragma once
#include "protocol.hpp"
#include "pe.hpp"
#include <windows.h>
#include <wintrust.h>
#include <softpub.h>
#include <wincrypt.h>
#include <bcrypt.h>
#include <filesystem>
#include <memory>
#include <iostream>
#include <iomanip>
#include <sstream>
#include <cwctype>
#include <cctype>
#include <map>
#include <tlhelp32.h>
namespace omsinr {
inline std::string file_version(const std::filesystem::path& path) {
    DWORD unused{};const DWORD size=GetFileVersionInfoSizeW(path.c_str(),&unused);
    if(!size)return "unavailable";
    std::vector<uint8_t> bytes(size);VS_FIXEDFILEINFO* info{};UINT length{};
    if(!GetFileVersionInfoW(path.c_str(),0,size,bytes.data()) ||
       !VerQueryValueW(bytes.data(),L"\\",reinterpret_cast<void**>(&info),&length) ||
       length<sizeof(*info) || info->dwSignature!=0xfeef04bd)return "unavailable";
    return std::to_string(HIWORD(info->dwFileVersionMS))+"."+std::to_string(LOWORD(info->dwFileVersionMS))+"."+
        std::to_string(HIWORD(info->dwFileVersionLS))+"."+std::to_string(LOWORD(info->dwFileVersionLS));
}
inline std::string code_hex(uint32_t code) {std::ostringstream out;out<<"0x"<<std::hex<<std::setw(8)<<std::setfill('0')<<code;return out.str();}
inline void hr(HRESULT r,const char* operation) {if(FAILED(r))throw std::runtime_error(std::string(operation)+" HRESULT="+code_hex(uint32_t(r))+" ("+std::to_string(int32_t(r))+")");}
inline void win32_check(bool ok,const char* operation) {
    if(!ok) {const DWORD code=GetLastError();throw std::runtime_error(std::string(operation)+" GetLastError="+std::to_string(code)+" "+code_hex(code));}
}
inline void check_ngx_profile(const std::filesystem::path& dll) {
    const auto version=file_version(dll);
    require(version.rfind("310.8.0.",0)==0 || version.rfind("310.9.16.",0)==0,
        "VersionProfile: NGX private profile requires 310.8.0.* or 310.9.16.*; unknown version refused");
}
inline void inspect_dependencies(const std::filesystem::path& path,const std::vector<Import>& imports) {
    wchar_t system[32768]{};const auto count=GetSystemDirectoryW(system,32768);win32_check(count>0 && count<32768,"GetSystemDirectoryW");
    for(const auto& imported:imports) {
        auto lower=imported.name;for(auto& c:lower)c=char(std::tolower(static_cast<unsigned char>(c)));
        const auto name=std::filesystem::u8path(imported.name);
        std::filesystem::path resolved;
        if(std::filesystem::is_regular_file(path.parent_path()/name))resolved=path.parent_path()/name;
        else if(std::filesystem::is_regular_file(std::filesystem::path(system)/name))resolved=std::filesystem::path(system)/name;
        else if(const auto module=GetModuleHandleW(name.c_str())) {
            wchar_t module_path[32768]{};const auto size=GetModuleFileNameW(module,module_path,32768);
            if(size && size<32768)resolved=module_path;
        }
        const bool api_set=lower.rfind("api-ms-",0)==0 || lower.rfind("ext-ms-",0)==0;
        std::cout<<"dependency="<<imported.name<<" delayed="<<imported.delayed<<" resolution="
            <<(!resolved.empty()?resolved.u8string():api_set?"Windows API-set contract; loader resolves":"missing")<<'\n';
        if(resolved.empty() && !api_set && !imported.delayed)
            throw std::runtime_error("MissingDependency: "+imported.name+" required by "+path.filename().u8string());
    }
    std::cout<<"Static imports inspected; dynamic/optional dependencies are decided by the loader/API, not this inventory.\n";
}
struct Handle {HANDLE h=INVALID_HANDLE_VALUE;~Handle(){if(h && h!=INVALID_HANDLE_VALUE)CloseHandle(h);}};
inline void verify_nr_origin(const std::filesystem::path& expected) {
    Handle snapshot;snapshot.h=CreateToolhelp32Snapshot(TH32CS_SNAPMODULE|TH32CS_SNAPMODULE32,GetCurrentProcessId());
    require(snapshot.h!=INVALID_HANDLE_VALUE,"NR loaded-module inventory failed");
    MODULEENTRY32W module{};module.dwSize=sizeof(module);bool found=false;
    require(Module32FirstW(snapshot.h,&module)!=0,"NR loaded-module enumeration failed");
    do {
        if(_wcsicmp(module.szModule,L"nvngx_dlssnr.dll")==0) {
            require(std::filesystem::equivalent(expected,std::filesystem::path(module.szExePath)),"dispatcher loaded another NR runtime; refusing evaluation");
            found=true;
        }
    }while(Module32NextW(snapshot.h,&module));
    require(found,"NR module missing after initialization/evaluation");
}

// Keep each verified DLL open without FILE_SHARE_WRITE/DELETE until shutdown.
// Trust is checked by Windows, not inferred from a GitHub asset checksum.
struct RuntimeFiles {
    std::vector<std::unique_ptr<Handle>> locks;
    std::map<std::wstring,std::string> digests;
    RuntimeFiles(const std::filesystem::path& runtime,const char* single_file) {verify(runtime/single_file);}
    static std::string sha256(HANDLE file) {
        BCRYPT_ALG_HANDLE algorithm{};BCRYPT_HASH_HANDLE hash{};
        require(BCryptOpenAlgorithmProvider(&algorithm,BCRYPT_SHA256_ALGORITHM,nullptr,0)==0,"SHA256 provider failed");
        struct Cleanup {BCRYPT_ALG_HANDLE a;BCRYPT_HASH_HANDLE* h;~Cleanup(){if(*h)BCryptDestroyHash(*h);BCryptCloseAlgorithmProvider(a,0);}} cleanup{algorithm,&hash};
        require(BCryptCreateHash(algorithm,&hash,nullptr,0,nullptr,0,0)==0,"SHA256 initialization failed");
        LARGE_INTEGER zero{};hr(SetFilePointerEx(file,zero,nullptr,FILE_BEGIN)?S_OK:E_FAIL,"hash seek failed");
        std::array<UCHAR,65536> buffer{};DWORD count{};
        do {require(ReadFile(file,buffer.data(),DWORD(buffer.size()),&count,nullptr)!=0,"hash read failed");
            if(count)require(BCryptHashData(hash,buffer.data(),count,0)==0,"hash update failed");}while(count);
        std::array<UCHAR,32> digest{};require(BCryptFinishHash(hash,digest.data(),DWORD(digest.size()),0)==0,"hash finish failed");
        std::ostringstream text;text<<std::hex<<std::setfill('0');for(auto b:digest)text<<std::setw(2)<<unsigned(b);return text.str();
    }
    static std::string sha256_parts(std::initializer_list<std::pair<const uint8_t*,size_t>> parts) {
        BCRYPT_ALG_HANDLE algorithm{};BCRYPT_HASH_HANDLE hash{};
        require(BCryptOpenAlgorithmProvider(&algorithm,BCRYPT_SHA256_ALGORITHM,nullptr,0)==0,"SHA256 provider failed");
        struct Cleanup {BCRYPT_ALG_HANDLE a;BCRYPT_HASH_HANDLE* h;~Cleanup(){if(*h)BCryptDestroyHash(*h);BCryptCloseAlgorithmProvider(a,0);}} cleanup{algorithm,&hash};
        require(BCryptCreateHash(algorithm,&hash,nullptr,0,nullptr,0,0)==0,"SHA256 initialization failed");
        for(const auto& part:parts) {
            require(part.second<=0xffffffffu,"SHA256 part too large");
            require(BCryptHashData(hash,const_cast<PUCHAR>(part.first),ULONG(part.second),0)==0,"SHA256 update failed");
        }
        std::array<UCHAR,32> digest{};require(BCryptFinishHash(hash,digest.data(),DWORD(digest.size()),0)==0,"SHA256 finish failed");
        std::ostringstream text;text<<std::hex<<std::setfill('0');for(auto b:digest)text<<std::setw(2)<<unsigned(b);return text.str();
    }
    void verify(const std::filesystem::path& path) {
        const auto attributes=GetFileAttributesW(path.c_str());
        win32_check(attributes!=INVALID_FILE_ATTRIBUTES,"GetFileAttributesW(runtime)");
        require(!(attributes&FILE_ATTRIBUTE_REPARSE_POINT),"runtime reparse point refused");
        auto lock=std::make_unique<Handle>();
        lock->h=CreateFileW(path.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,nullptr);
        win32_check(lock->h!=INVALID_HANDLE_VALUE,"CreateFileW(runtime, read-lock)");
        const auto digest=sha256(lock->h);auto key=path.filename().wstring();
        for(auto& c:key)c=std::towlower(c);digests[key]=digest;
        std::cout<<"unverified candidate="<<path.filename().u8string()<<" version="<<file_version(path)<<" sha256="<<digest<<'\n';
        std::ifstream pe(path,std::ios::binary);
        const auto imports=PeImports(pe,std::filesystem::file_size(path)).inspect();pe.close();
        WINTRUST_FILE_INFO info{};info.cbStruct=sizeof info;info.pcwszFilePath=path.c_str();info.hFile=lock->h;
        WINTRUST_DATA data{};data.cbStruct=sizeof data;data.dwUIChoice=WTD_UI_NONE;
        data.fdwRevocationChecks=WTD_REVOKE_WHOLECHAIN;data.dwUnionChoice=WTD_CHOICE_FILE;data.pFile=&info;
        data.dwStateAction=WTD_STATEACTION_VERIFY;
        data.dwProvFlags=WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT|WTD_CACHE_ONLY_URL_RETRIEVAL;
        GUID action=WINTRUST_ACTION_GENERIC_VERIFY_V2;
        const LONG trust=WinVerifyTrust(nullptr,&action,&data);
        data.dwStateAction=WTD_STATEACTION_CLOSE;WinVerifyTrust(nullptr,&action,&data);
        if(trust!=ERROR_SUCCESS)throw std::runtime_error("WinVerifyTrust Authenticode="+code_hex(uint32_t(trust))+" (including unavailable revocation evidence)");
        HCERTSTORE store{};HCRYPTMSG message{};DWORD encoding{},content{},format{};
        require(CryptQueryObject(CERT_QUERY_OBJECT_FILE,path.c_str(),CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
            CERT_QUERY_FORMAT_FLAG_BINARY,0,&encoding,&content,&format,&store,&message,nullptr)!=0,"embedded signature unavailable");
        struct CryptoCleanup {HCERTSTORE s;HCRYPTMSG m;~CryptoCleanup(){CryptMsgClose(m);CertCloseStore(s,0);}} cleanup{store,message};
        DWORD bytes{};require(CryptMsgGetParam(message,CMSG_SIGNER_INFO_PARAM,0,nullptr,&bytes)!=0,"signer missing");
        std::vector<uint8_t> signer(bytes);require(CryptMsgGetParam(message,CMSG_SIGNER_INFO_PARAM,0,signer.data(),&bytes)!=0,"signer read failed");
        const auto* si=reinterpret_cast<const CMSG_SIGNER_INFO*>(signer.data());CERT_INFO wanted{};
        wanted.Issuer=si->Issuer;wanted.SerialNumber=si->SerialNumber;
        const auto cert=CertFindCertificateInStore(store,encoding,0,CERT_FIND_SUBJECT_CERT,&wanted,nullptr);
        require(cert!=nullptr,"signer certificate not found");wchar_t name[512]{};
        CertGetNameStringW(cert,CERT_NAME_SIMPLE_DISPLAY_TYPE,0,nullptr,name,512);CertFreeCertificateContext(cert);
        require(std::wstring(name)==L"NVIDIA Corporation","runtime signer is not NVIDIA Corporation");
        inspect_dependencies(path,imports);
        std::cout<<"verified version="<<file_version(path)<<" runtime="<<path.filename().u8string()<<" sha256="<<digest<<'\n';
        locks.push_back(std::move(lock));
    }
    explicit RuntimeFiles(const std::filesystem::path& runtime, bool ngx=false) {
        for(const auto* name:{L"sl.interposer.dll",L"sl.common.dll",L"sl.dlss_nr.dll",L"nvngx_dlssnr.dll"})
            if((!ngx || std::wstring(name)==L"nvngx_dlssnr.dll") && !std::filesystem::is_regular_file(runtime/name))throw std::runtime_error("required NR runtime missing: "+(runtime/name).u8string());
        for(const auto& e:std::filesystem::directory_iterator(runtime)) {
            auto extension=e.path().extension().wstring();for(auto& c:extension)c=std::towlower(c);
            if(extension==L".dll") {require(!e.is_symlink(),"runtime symlink refused");verify(e.path());}
        }
    }
};

}
