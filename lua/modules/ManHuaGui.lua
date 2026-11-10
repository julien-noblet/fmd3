local js = require 'utils.jsunpack'
local lz = require 'utils.lzstring'

function Init()
	local m = NewWebsiteModule()
	m.ID                        = '39e7f3efe2fd43c6ad0abc68b054cfc7'
	m.Name                      = 'ManHuaGui'
	m.RootURL                   = 'https://www.manhuagui.com'
	m.Category                  = 'Raw'
	m.OnGetDirectoryPageNumber  = 'GetDirectoryPageNumber'
	m.OnGetNameAndLink          = 'GetNameAndLink'
	m.OnGetInfo                 = 'GetInfo'
	m.OnGetPageNumber           = 'GetPageNumber'
	m.OnBeforeDownloadImage     = 'BeforeDownloadImage'
end

function GetDirectoryPageNumber()
	Delay()
	if HTTP.GET(MODULE.RootURL .. '/list/') then
		x = CreateTXQuery(HTTP.Document)
		PAGENUMBER = tonumber(x.XPathString('//div[contains(@id, "AspNetPager")]/a[last()]/@href'):match('%d+')) or 1
		return no_error
	else
		return net_problem
	end
end

function GetNameAndLink()
	Delay()
	if HTTP.GET(MODULE.RootURL .. '/list/index_p' .. (URL + 1) .. '.html') then
		CreateTXQuery(HTTP.Document).XPathHREFAll('//ul[@id="contList"]/li/p/a', LINKS, NAMES)
		return no_error
	else
		return net_problem
	end
end

function GetInfo()
	Delay()
	MANGAINFO.URL = MaybeFillHost(MODULE.RootURL, URL)
	HTTP.Cookies.Values['isAdult']=' 1'
	if HTTP.GET(MANGAINFO.URL) then
		local x = CreateTXQuery(HTTP.Document)

		MANGAINFO.CoverLink = MaybeFillHost(MODULE.RootURL,x.XPathString('//p[@class="hcover"]/img/@src'))
		MANGAINFO.Title     = x.XPathString('//div[@class="book-title"]/h1')
		MANGAINFO.Authors   = SeparateRight(x.XPathString('//ul[@class="detail-list cf"]/li[2]/span[2]'), '：')
		MANGAINFO.Genres    = SeparateRight(x.XPathString('//ul[@class="detail-list cf"]/li[2]/span[1]'), '：')
		MANGAINFO.Status    = MangaInfoStatusIfPos(x.XPathString('//ul[@class="detail-list cf"]/li[@class="status"]'), '连载中')
		MANGAINFO.Summary   = x.XPathString('//div[@id="intro-all"]')

		if x.XPath('//*[@id="__VIEWSTATE"]').Count ~= 0 then
			local s = x.XPathString('//*[contains(@class,"chapter")]/input/@value')
			if s~='' then x.ParseHTML(lz.decompressFromBase64(s)) end
		end
		x.XPathHREFTitleAll('//*[contains(@id,"chapter-list")]/ul/li/a', MANGAINFO.ChapterLinks, MANGAINFO.ChapterNames)
		MANGAINFO.ChapterLinks.Reverse(); MANGAINFO.ChapterNames.Reverse()
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	local servers = {
		'http://i.hamreus.com',
		'http://us.hamreus.com',
		'http://dx.hamreus.com',
		'http://eu.hamreus.com',
		'http://lt.hamreus.com',
	}

	math.randomseed(os.time())
	math.random(); math.random(); math.random();

	if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
		local x = CreateTXQuery(HTTP.Document)
		local s = x.XPathString('//script[contains(., "p,a,c,k")]')
		s = SeparateRight(s, "}('")
		local text = SeparateLeft(s, "',");
		local a = tonumber(GetBetween("',", ",", s))
		s = SeparateRight(s, "',")
		local c = tonumber(GetBetween(",", ",'", s))
		local w = js.splitstr(lz.decompressFromBase64(GetBetween(",'", "'", s)), '|')
		s = js.unpack36(text, a, c, w)
		s = s:gsub('^var%s+.+=%s*{', '{'):gsub('||{};$', ''):gsub('"status":,', '')
		s = GetBetween("SMH.imgData(", ").preInit();", s)
		x.ParseHTML(s)
		local cid = x.XPathString('json(*).cid')
		local md5 = x.XPathString('json(*).sl.md5')
		local path = x.XPathString('json(*).path')
		local srv = servers[math.random(#servers)]
		local v for v in x.XPath('json(*).files()').Get() do
			TASK.PageLinks.Add(srv .. path .. v.ToString() .. '?cid=' .. cid .. '&md5=' .. md5)
		end
		return true
	else
		return false
	end
end

function BeforeDownloadImage()
	HTTP.Headers.Values['Referer'] = MODULE.RootURL
	return true
end

function Delay()
	local lastDelay = tonumber(MODULE.Storage['lastDelay']) or 1
	local mhg_delay = tonumber(MODULE.GetOption('mhg_delay')) or 5 -- * MODULE.ActiveConnectionCount
	if lastDelay ~= '' then
		lastDelay = os.time() - lastDelay
		if lastDelay < mhg_delay then
			sleep((mhg_delay - lastDelay) * 1000)
		end
	end
	MODULE.Storage['lastDelay'] = os.time()
end
