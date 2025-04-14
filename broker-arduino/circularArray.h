#pragma once

#include <array>

template<typename T, int maxSize>
class CircularArray final
{
public:
    CircularArray()
    {
        data.fill(T{});
    }

    void push(const T& value)
    {
        data.at(head) = value;
        head = (head + 1) % maxSize;
        if (count < maxSize) {
            ++count;
        } else {
            tail = (tail + 1) % maxSize;
        }
    }

    const T& at(int index) const
    {
        return data.at((tail + index) % maxSize);
    }

    int size() const
    {
        return count;
    }

    void clear()
    {
        data.fill(T{});
        head = 0;
        tail = 0;
        count = 0;
    }

    bool full() const
    {
        return count == maxSize;
    }

    class ConstIterator final
    {
    public:
        explicit ConstIterator(const CircularArray& array, int pos)
            : array(array)
            , pos(pos)
        {}

        const T& operator*() const
        {
            return array.at(pos);
        }
        ConstIterator& operator++()
        {
            ++pos;
            return *this;
        }
        bool operator!=(const ConstIterator& other) const
        {
            return pos != other.pos;
        }

    private:
        const CircularArray& array;
        int pos;
    };

    ConstIterator begin() const
    {
        return ConstIterator(*this, 0);
    }

    ConstIterator end() const
    {
        return ConstIterator(*this, count);
    }

private:
    std::array<T, maxSize> data;
    int head = 0;
    int tail = 0;
    int count = 0;
};