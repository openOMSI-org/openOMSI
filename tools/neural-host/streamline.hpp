#pragma once
#include "protocol.hpp"
#include "evidence.hpp"
#include <filesystem>
#include <memory>
#include <string>
namespace omsinr {
enum class Backend { Streamline, Ngx };
bool ngx_compiled();
class NeuralHost {
    struct Impl;
    std::unique_ptr<Impl> impl;
public:
    NeuralHost(const std::filesystem::path& runtime,const std::string& project,
               Backend backend,Evidence& evidence,const std::filesystem::path& data);
    ~NeuralHost();
    std::vector<uint8_t> evaluate(const Frame& frame,uint32_t& stages,int32_t& result);
};
}
